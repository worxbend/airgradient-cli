use std::{
    io::{self, Stdout},
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use thiserror::Error;
use url::Url;

use crate::{
    config::{self, MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS},
    device::{self, FetchSettings},
    sensors,
    tui::app::TuiApp,
};

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub config_path: PathBuf,
    pub url_override: Option<String>,
    pub refresh_override_secs: Option<u64>,
    pub fetch_settings: FetchSettings,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Device(#[from] device::DeviceError),

    #[error("terminal I/O failed")]
    Terminal(#[from] io::Error),
}

pub async fn run(options: RuntimeOptions) -> Result<(), RuntimeError> {
    let effective_config = EffectiveConfig::resolve(&options);
    let fetch_client = options.fetch_settings.client()?;
    let mut app = TuiApp::with_client(
        effective_config.configured_url,
        effective_config.refresh_interval,
        options.fetch_settings,
        fetch_client,
    );

    if let Some(error) = effective_config.current_error {
        app.current_error = Some(error);
    }

    let mut terminal = TerminalSession::enter()?;
    let result = run_loop(&mut terminal, &mut app).await;
    let restore_result = terminal.restore();

    result?;
    restore_result?;

    Ok(())
}

async fn run_loop(terminal: &mut TerminalSession, app: &mut TuiApp) -> Result<(), RuntimeError> {
    if app.configured_url.is_some() {
        refresh(app).await;
    }

    terminal.draw(app)?;
    let mut next_refresh = Instant::now() + app.refresh_interval;

    loop {
        let now = Instant::now();
        let poll_timeout = next_refresh.saturating_duration_since(now);

        if event::poll(poll_timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        if app.configured_url.is_some() {
                            refresh(app).await;
                        }
                        next_refresh = Instant::now() + app.refresh_interval;
                    }
                    _ => {}
                }

                terminal.draw(app)?;
            }

            continue;
        }

        if app.configured_url.is_some() {
            refresh(app).await;
        }
        next_refresh = Instant::now() + app.refresh_interval;
        terminal.draw(app)?;
    }

    Ok(())
}

async fn refresh(app: &mut TuiApp) {
    let Some(base_url) = app.configured_url.clone() else {
        return;
    };

    let started = Instant::now();
    match device::fetch_current_measures_with_client(&app.fetch_client, &base_url).await {
        Ok(payload) => {
            let snapshot = sensors::parse_snapshot(&payload);
            app.apply_success(snapshot, started.elapsed(), SystemTime::now());
        }
        Err(error) => {
            app.apply_failure(error, started.elapsed());
        }
    }
}

#[derive(Debug)]
struct EffectiveConfig {
    configured_url: Option<Url>,
    refresh_interval: Duration,
    current_error: Option<String>,
}

impl EffectiveConfig {
    fn resolve(options: &RuntimeOptions) -> Self {
        let mut current_error = None;
        let mut configured_url = None;

        let config = match config::read_display_config(&options.config_path) {
            Ok(display) => {
                if !display.warnings.is_empty() {
                    current_error = Some(display.warnings.join("; "));
                }
                display.config
            }
            Err(error) => {
                current_error = Some(error.to_string());
                config::Config::default()
            }
        };

        let server_url = options
            .url_override
            .as_deref()
            .map(str::to_owned)
            .or(config.server_url);

        if let Some(server_url) = server_url.as_deref().filter(|url| !url.trim().is_empty()) {
            match device::normalize_base_url(server_url) {
                Ok(url) => configured_url = Some(url),
                Err(error) => current_error = Some(error.to_string()),
            }
        }

        let refresh_secs = options
            .refresh_override_secs
            .unwrap_or(config.refresh_interval_secs);

        Self {
            configured_url,
            refresh_interval: Duration::from_secs(clamp_refresh_secs(refresh_secs)),
            current_error,
        }
    }
}

fn clamp_refresh_secs(seconds: u64) -> u64 {
    seconds.clamp(MIN_REFRESH_INTERVAL_SECS, MAX_REFRESH_INTERVAL_SECS)
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    raw_mode_enabled: bool,
    alternate_screen_enabled: bool,
}

impl TerminalSession {
    fn enter() -> Result<Self, RuntimeError> {
        enable_raw_mode()?;
        let raw_mode_enabled = true;

        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }

        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                return Err(error.into());
            }
        };

        Ok(Self {
            terminal,
            raw_mode_enabled,
            alternate_screen_enabled: true,
        })
    }

    fn draw(&mut self, app: &TuiApp) -> Result<(), RuntimeError> {
        self.terminal
            .draw(|frame| crate::tui::ui::draw(frame, app))?;
        Ok(())
    }

    fn restore(&mut self) -> Result<(), RuntimeError> {
        let mut first_error = None;
        let cleanup_plan =
            terminal_cleanup_plan(self.raw_mode_enabled, self.alternate_screen_enabled);

        if cleanup_plan.disable_raw_mode {
            if let Err(error) = disable_raw_mode() {
                first_error = Some(error);
            } else {
                self.raw_mode_enabled = false;
            }
        }

        if cleanup_plan.leave_alternate_screen {
            if let Err(error) = execute!(self.terminal.backend_mut(), LeaveAlternateScreen) {
                first_error.get_or_insert(error);
            } else {
                self.alternate_screen_enabled = false;
            }
        }

        if cleanup_plan.show_cursor
            && let Err(error) = self.terminal.show_cursor()
        {
            first_error.get_or_insert(error);
        }

        match first_error {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalCleanupPlan {
    disable_raw_mode: bool,
    leave_alternate_screen: bool,
    show_cursor: bool,
}

fn terminal_cleanup_plan(
    raw_mode_enabled: bool,
    alternate_screen_enabled: bool,
) -> TerminalCleanupPlan {
    TerminalCleanupPlan {
        disable_raw_mode: raw_mode_enabled,
        leave_alternate_screen: alternate_screen_enabled,
        show_cursor: raw_mode_enabled || alternate_screen_enabled,
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_plan_restores_resources_after_terminal_setup_started() {
        assert_eq!(
            terminal_cleanup_plan(true, true),
            TerminalCleanupPlan {
                disable_raw_mode: true,
                leave_alternate_screen: true,
                show_cursor: true,
            }
        );
    }

    #[test]
    fn cleanup_plan_handles_partial_setup() {
        assert_eq!(
            terminal_cleanup_plan(true, false),
            TerminalCleanupPlan {
                disable_raw_mode: true,
                leave_alternate_screen: false,
                show_cursor: true,
            }
        );
        assert_eq!(
            terminal_cleanup_plan(false, false),
            TerminalCleanupPlan {
                disable_raw_mode: false,
                leave_alternate_screen: false,
                show_cursor: false,
            }
        );
    }
}
