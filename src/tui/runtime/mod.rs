//! Owns the TUI process lifecycle: resolve configuration, take over the
//! terminal, run the event loop, and restore the terminal on the way out.
//!
//! The pieces are split so each has one reason to change:
//!
//! - [`effective_config`] — merging config file, CLI overrides, and defaults
//! - [`terminal`] — the crossterm/ratatui adapter and its teardown ordering
//! - [`fetch`] — scheduling background measurement fetches
//! - [`event_loop`] — the draw/poll/refresh loop that drives them
//!
//! Everything below the top-level [`run`] is written against the
//! [`terminal::TerminalRuntime`] and [`fetch::MeasureFetchWorker`] traits, so
//! the loop is tested with in-memory fakes and no real terminal.

mod effective_config;
mod event_loop;
mod fetch;
mod terminal;

#[cfg(test)]
mod tests;

use std::{
    env,
    io::{self, IsTerminal},
    path::PathBuf,
    time::Duration,
};

use thiserror::Error;

use crate::{
    device::{self, FetchSettings},
    tui::app::TuiApp,
};

use effective_config::{EffectiveConfig, runtime_refresh_schedule_interval};
use event_loop::{RuntimeEvent, SystemClock, run_loop, run_splash};
use fetch::{BackgroundMeasureFetcher, MeasureFetchWorker};
use terminal::{CrosstermRuntime, TerminalRuntime};

pub use effective_config::TUI_TEST_REFRESH_INTERVAL_MS_ENV;

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub config_path: PathBuf,
    pub url_override: Option<String>,
    pub refresh_override_secs: Option<u64>,
    pub theme_override: Option<String>,
    pub fetch_settings: FetchSettings,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Device(#[from] device::DeviceError),

    #[error("terminal I/O failed")]
    Terminal(#[from] io::Error),

    #[error("TUI requires an interactive terminal")]
    NonInteractiveTerminal,

    #[error("background fetch task failed: {0}")]
    FetchTask(String),

    /// A failure that happened while already unwinding from another one, so
    /// neither message is lost when diagnosing a shutdown.
    #[error("{primary} (secondary runtime error: {secondary})")]
    Secondary {
        primary: Box<RuntimeError>,
        secondary: Box<RuntimeError>,
    },

    #[error("{primary} (terminal cleanup also failed: {cleanup})")]
    Cleanup {
        primary: Box<RuntimeError>,
        cleanup: io::Error,
    },
}

impl RuntimeError {
    pub(super) fn with_secondary_failure(primary: RuntimeError, secondary: RuntimeError) -> Self {
        RuntimeError::Secondary {
            primary: Box::new(primary),
            secondary: Box::new(secondary),
        }
    }

    /// Flattens a cleanup failure into an `io::Error` so the `Cleanup` variant
    /// stays a leaf: cleanup errors are always reported alongside the primary
    /// failure, never as the head of another error chain.
    fn with_cleanup_failure(primary: RuntimeError, cleanup: RuntimeError) -> Self {
        let cleanup = match cleanup {
            RuntimeError::Terminal(error) => error,
            RuntimeError::FetchTask(message) => io::Error::other(message),
            other => io::Error::other(other.to_string()),
        };

        RuntimeError::Cleanup {
            primary: Box::new(primary),
            cleanup,
        }
    }
}

pub async fn run(options: RuntimeOptions) -> Result<(), RuntimeError> {
    ensure_interactive_terminal()?;

    let effective_config = EffectiveConfig::resolve(&options);
    let fetch_client = options.fetch_settings.client()?;
    let mut app = TuiApp::new(
        effective_config.configured_url,
        effective_config.refresh_interval,
    );
    app.theme = effective_config.theme;
    app.config_path = Some(options.config_path.clone());
    let refresh_schedule_interval = runtime_refresh_schedule_interval(
        app.refresh_interval,
        env::var(TUI_TEST_REFRESH_INTERVAL_MS_ENV).ok().as_deref(),
    );

    if let Some(error) = effective_config.current_error {
        app.current_error = Some(error);
    }

    let mut terminal = CrosstermRuntime::default();
    let mut fetcher = BackgroundMeasureFetcher::new(fetch_client);

    run_with_adapters_with_refresh_interval(
        &mut terminal,
        &mut app,
        &mut fetcher,
        refresh_schedule_interval,
        true,
    )
    .await
}

/// Both stdout and stderr must be TTYs: the alternate screen goes to stdout,
/// but a redirected stderr would still let panics and errors scribble over it.
fn ensure_interactive_terminal() -> Result<(), RuntimeError> {
    if io::stdout().is_terminal() && io::stderr().is_terminal() {
        Ok(())
    } else {
        Err(RuntimeError::NonInteractiveTerminal)
    }
}

#[cfg(test)]
async fn run_with_adapters<T, F>(
    terminal: &mut T,
    app: &mut TuiApp,
    fetcher: &mut F,
) -> Result<(), RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
{
    let refresh_schedule_interval = app.refresh_interval;
    run_with_adapters_with_refresh_interval(
        terminal,
        app,
        fetcher,
        refresh_schedule_interval,
        false,
    )
    .await
}

/// Runs the loop between terminal setup and teardown. Cleanup runs even when
/// the loop failed, and a cleanup failure never masks the original error —
/// see [`RuntimeError::with_cleanup_failure`].
async fn run_with_adapters_with_refresh_interval<T, F>(
    terminal: &mut T,
    app: &mut TuiApp,
    fetcher: &mut F,
    refresh_schedule_interval: Duration,
    show_splash: bool,
) -> Result<(), RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
{
    terminal.enter()?;

    let priming_event: Option<RuntimeEvent> = if show_splash {
        run_splash(terminal, app).await?
    } else {
        None
    };

    let mut clock = SystemClock;
    let result = run_loop(
        terminal,
        app,
        fetcher,
        &mut clock,
        refresh_schedule_interval,
        priming_event,
    )
    .await;
    let cleanup_result = terminal.cleanup();

    match (result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => Err(RuntimeError::with_cleanup_failure(primary, cleanup)),
    }
}
