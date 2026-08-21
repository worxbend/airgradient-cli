//! Owns the TUI process lifecycle: resolve configuration, take over the
//! terminal, run the event loop, and restore the terminal on the way out.
//!
//! The pieces are split so each has one reason to change:
//!
//! - [`effective_config`] — merging config file, CLI overrides, and defaults
//! - [`terminal`] — the crossterm/ratatui adapter and its teardown ordering
//! - [`event`] — what a keypress means in the currently active mode
//! - [`fetch`] — scheduling background measurement fetches
//! - [`schedule`] — when the next automatic refresh is due
//! - [`event_loop`] — the draw/poll/refresh loop that drives them
//!
//! Everything below the top-level [`run`] is written against the
//! [`terminal::TerminalRuntime`] and [`fetch::MeasureFetchWorker`] traits, so
//! the loop is tested with in-memory fakes and no real terminal.

mod effective_config;
mod event;
mod event_loop;
mod fetch;
mod keymap;
mod schedule;
mod terminal;
mod view_events;

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
    device_url::DeviceUrlError,
    history,
    tui::app::TuiApp,
};

/// How much recorded history to replay into a new session.
///
/// The chart plots what fits on screen and the in-memory buffer is smaller
/// still, so reading the whole file would only be work thrown away.
const MAX_REPLAYED_READINGS: usize = 512;

use effective_config::{EffectiveConfig, runtime_refresh_schedule_interval};
use event::RuntimeEvent;
use event_loop::{run_loop, run_splash};
use fetch::{BackgroundMeasureFetcher, MeasureFetchWorker};
use schedule::SystemClock;
use terminal::{CrosstermRuntime, TerminalRuntime};

pub use effective_config::TUI_TEST_REFRESH_INTERVAL_MS_ENV;

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub config_path: PathBuf,
    /// Where readings are recorded for the chart. `None` disables recording
    /// entirely, which is what a session with no resolvable data directory
    /// gets — the dashboard still works, it just has no history to draw.
    pub history_path: Option<PathBuf>,
    pub url_override: Option<String>,
    pub refresh_override_secs: Option<u64>,
    pub theme_override: Option<String>,
    pub fetch_settings: FetchSettings,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Device(#[from] device::DeviceError),

    #[error(transparent)]
    DeviceUrl(#[from] DeviceUrlError),

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
    app.history_path = options.history_path.clone();

    // Previously recorded readings, so the chart opens with a shape rather
    // than filling in over the next quarter of an hour. Best-effort: an
    // unreadable history costs the graph, not the dashboard.
    if let Some(path) = &options.history_path
        && let Ok(readings) = history::load_recent(path, MAX_REPLAYED_READINGS)
    {
        app.restore_history(readings);
    }
    let refresh_schedule_interval = runtime_refresh_schedule_interval(
        app.refresh_interval.as_duration(),
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
    let refresh_schedule_interval = app.refresh_interval.as_duration();
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

    // The splash is inside the same result as the loop, not returned with `?`
    // ahead of it. Once `enter` succeeds the terminal is in raw mode on the
    // alternate screen with mouse capture on, so any early return from here to
    // `cleanup` would hand the user back a terminal in that state.
    let mut clock = SystemClock;
    let splash_result: Result<Option<RuntimeEvent>, RuntimeError> = if show_splash {
        run_splash(terminal, app).await
    } else {
        Ok(None)
    };
    let result = match splash_result {
        Ok(priming_event) => {
            run_loop(
                terminal,
                app,
                fetcher,
                &mut clock,
                refresh_schedule_interval,
                priming_event,
            )
            .await
        }
        Err(error) => Err(error),
    };
    let cleanup_result = terminal.cleanup();

    match (result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => Err(RuntimeError::with_cleanup_failure(primary, cleanup)),
    }
}
