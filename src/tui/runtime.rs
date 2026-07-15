use std::{
    env,
    io::{self, IsTerminal, Stdout},
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use reqwest::Client;
use serde_json::Value;
use thiserror::Error;
use tokio::task::JoinHandle;
use url::Url;

use crate::{
    config,
    device::{self, FetchSettings},
    sensors,
    tui::{
        app::{PaletteOutcome, TuiApp, View},
        theme::{self, Theme},
    },
};

const FETCH_RESULT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const MIN_TUI_TEST_REFRESH_INTERVAL_MS: u64 = 100;
const MIN_TUI_TEST_REFRESH_INTERVAL: Duration =
    Duration::from_millis(MIN_TUI_TEST_REFRESH_INTERVAL_MS);
#[doc(hidden)]
/// Diagnostic/test-only scheduler override. Values below 100ms are ignored.
pub const TUI_TEST_REFRESH_INTERVAL_MS_ENV: &str = "AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS";

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

    let priming_event = if show_splash {
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

impl RuntimeError {
    fn with_secondary_failure(primary: RuntimeError, secondary: RuntimeError) -> Self {
        RuntimeError::Secondary {
            primary: Box::new(primary),
            secondary: Box::new(secondary),
        }
    }

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

async fn run_loop<T, F, C>(
    terminal: &mut T,
    app: &mut TuiApp,
    fetcher: &mut F,
    clock: &mut C,
    initial_refresh_schedule_interval: Duration,
    priming_event: Option<RuntimeEvent>,
) -> Result<(), RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
    C: RuntimeClock,
{
    let mut fetch_scheduler = FetchScheduler::default();
    let mut refresh_schedule_interval = initial_refresh_schedule_interval;
    let mut pending_priming = priming_event;

    let result = async {
        if app.configured_url.is_some() {
            fetch_scheduler.request_refresh(app, fetcher);
            fetch_scheduler.apply_ready_results(app, fetcher).await?;
        }

        terminal.draw(app)?;
        let mut now = clock.now();
        let mut next_refresh = now + refresh_schedule_interval;

        loop {
            if fetch_scheduler.apply_ready_results(app, fetcher).await? {
                terminal.draw(app)?;
            }

            now = clock.now();
            let time_until_refresh = next_refresh.saturating_duration_since(now);
            let poll_timeout = time_until_refresh.min(FETCH_RESULT_POLL_INTERVAL);

            // A splash-priming event (see `run_splash`) is consumed exactly
            // once, ahead of any real polling, so a keypress that skipped
            // the splash still takes effect on this very first iteration.
            let ready_event = if let Some(event) = pending_priming.take() {
                Some(event)
            } else if terminal.poll_event(poll_timeout).await? {
                let _poll_returned_at = clock.now();
                Some(terminal.read_event(input_mode(app)).await?)
            } else {
                None
            };

            if let Some(event) = ready_event {
                match event {
                    RuntimeEvent::Quit => break,
                    RuntimeEvent::Refresh => {
                        now = clock.now();
                        if app.configured_url.is_some() {
                            fetch_scheduler.request_refresh(app, fetcher);
                            fetch_scheduler.apply_ready_results(app, fetcher).await?;
                        }
                        next_refresh = now + refresh_schedule_interval;
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::IncreaseRefreshInterval => {
                        now = clock.now();
                        app.increase_refresh_interval();
                        refresh_schedule_interval = app.refresh_interval;
                        next_refresh = now + refresh_schedule_interval;
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::DecreaseRefreshInterval => {
                        now = clock.now();
                        app.decrease_refresh_interval();
                        refresh_schedule_interval = app.refresh_interval;
                        next_refresh = now + refresh_schedule_interval;
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::OpenPalette => {
                        app.open_command_palette();
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::ToggleThemeSettings => {
                        if app.view == View::ThemeSettings {
                            app.close_theme_settings();
                        } else {
                            app.open_theme_settings();
                        }
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::ToggleConfigEditor => {
                        if app.view == View::ConfigEditor {
                            app.close_config_editor();
                        } else {
                            app.open_config_editor();
                        }
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::Escape => {
                        match app.view {
                            View::ThemeSettings => app.close_theme_settings(),
                            View::ConfigEditor if app.config_editor_editing.is_some() => {
                                app.config_editor_cancel_edit();
                            }
                            View::ConfigEditor => app.close_config_editor(),
                            View::CommandPalette => app.close_command_palette(),
                            View::Dashboard => {}
                        }
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::NavUp => {
                        match app.view {
                            View::ThemeSettings => app.theme_cursor_up(),
                            View::ConfigEditor => app.config_editor_nav_up(),
                            View::CommandPalette | View::Dashboard => {}
                        }
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::NavDown => {
                        match app.view {
                            View::ThemeSettings => app.theme_cursor_down(),
                            View::ConfigEditor => app.config_editor_nav_down(),
                            View::CommandPalette | View::Dashboard => {}
                        }
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::Confirm => {
                        match app.view {
                            View::ThemeSettings => app.confirm_theme_settings(),
                            View::ConfigEditor => app.config_editor_confirm(),
                            View::CommandPalette => {
                                if app.palette_submit() == PaletteOutcome::Quit {
                                    break;
                                }
                            }
                            View::Dashboard => {}
                        }
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::PaletteChar(c) => {
                        match app.view {
                            View::CommandPalette => app.palette_push_char(c),
                            View::ConfigEditor => app.config_editor_push_char(c),
                            View::ThemeSettings | View::Dashboard => {}
                        }
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::PaletteBackspace => {
                        match app.view {
                            View::CommandPalette => app.palette_backspace(),
                            View::ConfigEditor => app.config_editor_backspace(),
                            View::ThemeSettings | View::Dashboard => {}
                        }
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::Ignored => {}
                }

                now = clock.now();
                if now >= next_refresh {
                    if app.configured_url.is_some() {
                        fetch_scheduler.request_refresh(app, fetcher);
                        fetch_scheduler.apply_ready_results(app, fetcher).await?;
                    }
                    next_refresh = now + refresh_schedule_interval;
                    terminal.draw(app)?;
                }

                continue;
            }

            now = clock.now();
            if now >= next_refresh {
                if app.configured_url.is_some() {
                    fetch_scheduler.request_refresh(app, fetcher);
                    fetch_scheduler.apply_ready_results(app, fetcher).await?;
                }
                next_refresh = now + refresh_schedule_interval;
                terminal.draw(app)?;
            }
        }

        Ok(())
    }
    .await;

    let cancel_result = fetch_scheduler.cancel_pending_fetch(app, fetcher).await;

    match (result, cancel_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Err(primary), Err(secondary)) => {
            Err(RuntimeError::with_secondary_failure(primary, secondary))
        }
        (Ok(()), Err(error)) => Err(error),
    }
}

trait RuntimeClock {
    fn now(&mut self) -> Instant;
}

struct SystemClock;

impl RuntimeClock for SystemClock {
    fn now(&mut self) -> Instant {
        Instant::now()
    }
}

#[derive(Debug, Default)]
struct FetchScheduler {
    in_flight: bool,
    follow_up_requested: bool,
}

impl FetchScheduler {
    fn request_refresh<F>(&mut self, app: &mut TuiApp, fetcher: &mut F)
    where
        F: MeasureFetchWorker,
    {
        let Some(base_url) = app.configured_url.clone() else {
            return;
        };

        if self.in_flight {
            self.follow_up_requested = true;
            return;
        }

        fetcher.start_fetch(base_url);
        self.in_flight = true;
        app.begin_fetch();
    }

    async fn apply_ready_results<F>(
        &mut self,
        app: &mut TuiApp,
        fetcher: &mut F,
    ) -> Result<bool, RuntimeError>
    where
        F: MeasureFetchWorker,
    {
        if !self.in_flight {
            while fetcher.recv_ready_fetch().await?.is_some() {}
            return Ok(false);
        }

        let mut applied_result = false;

        while let Some(completion) = fetcher.recv_ready_fetch().await? {
            self.in_flight = false;
            applied_result = true;

            match completion.result {
                Ok(payload) => {
                    let snapshot = sensors::parse_snapshot(&payload);
                    app.finish_fetch_success(
                        snapshot,
                        completion.duration,
                        completion.completed_at,
                    );
                }
                Err(error) => {
                    app.finish_fetch_failure(error, completion.duration);
                }
            }

            if self.follow_up_requested {
                self.follow_up_requested = false;
                self.request_refresh(app, fetcher);
            }
        }

        Ok(applied_result)
    }

    async fn cancel_pending_fetch<F>(
        &mut self,
        app: &mut TuiApp,
        fetcher: &mut F,
    ) -> Result<(), RuntimeError>
    where
        F: MeasureFetchWorker,
    {
        let cancel_result = if self.in_flight {
            fetcher.cancel_fetch().await
        } else {
            Ok(())
        };

        self.in_flight = false;
        self.follow_up_requested = false;
        app.is_fetching = false;
        cancel_result
    }
}

#[derive(Debug)]
struct FetchCompletion {
    result: Result<Value, String>,
    duration: Duration,
    completed_at: SystemTime,
}

trait MeasureFetchWorker {
    fn start_fetch(&mut self, base_url: Url);
    async fn recv_ready_fetch(&mut self) -> Result<Option<FetchCompletion>, RuntimeError>;
    async fn cancel_fetch(&mut self) -> Result<(), RuntimeError>;
}

struct BackgroundMeasureFetcher {
    client: Client,
    active_handle: Option<JoinHandle<FetchCompletion>>,
}

impl BackgroundMeasureFetcher {
    fn new(client: Client) -> Self {
        Self {
            client,
            active_handle: None,
        }
    }
}

impl MeasureFetchWorker for BackgroundMeasureFetcher {
    fn start_fetch(&mut self, base_url: Url) {
        if let Some(handle) = self.active_handle.take() {
            handle.abort();
        }

        let client = self.client.clone();

        self.active_handle = Some(tokio::spawn(async move {
            let started = Instant::now();
            let result = device::fetch_current_measures_with_client(&client, &base_url)
                .await
                .map_err(|error| error.to_string());
            FetchCompletion {
                result,
                duration: started.elapsed(),
                completed_at: SystemTime::now(),
            }
        }));
    }

    async fn recv_ready_fetch(&mut self) -> Result<Option<FetchCompletion>, RuntimeError> {
        let Some(handle) = self.active_handle.as_ref() else {
            return Ok(None);
        };

        if !handle.is_finished() {
            return Ok(None);
        }

        let handle = self
            .active_handle
            .take()
            .expect("active handle should exist after readiness check");
        observe_fetch_handle(handle).await.map(Some)
    }

    async fn cancel_fetch(&mut self) -> Result<(), RuntimeError> {
        if let Some(handle) = self.active_handle.take() {
            handle.abort();
            match handle.await {
                Ok(_) => {}
                Err(error) if error.is_cancelled() => {}
                Err(error) => return Err(RuntimeError::FetchTask(error.to_string())),
            }
        }

        Ok(())
    }
}

async fn observe_fetch_handle(
    handle: JoinHandle<FetchCompletion>,
) -> Result<FetchCompletion, RuntimeError> {
    handle.await.map_err(|error| {
        RuntimeError::FetchTask(if error.is_cancelled() {
            "fetch task was cancelled".to_owned()
        } else {
            error.to_string()
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeEvent {
    Quit,
    Refresh,
    IncreaseRefreshInterval,
    DecreaseRefreshInterval,
    /// Opens the `:` command palette.
    OpenPalette,
    /// Opens the theme picker if closed, closes it if open (`t`/`T`/`F2`).
    ToggleThemeSettings,
    /// Opens the config editor if closed, closes it if open (`c`/`C`).
    ToggleConfigEditor,
    /// Closes whichever modal view/field-edit is active; quitting on a bare
    /// `Esc` at the dashboard is still handled by the `Quit` variant above.
    Escape,
    NavUp,
    NavDown,
    /// Commits a field edit, applies a theme selection, or submits the
    /// palette line, depending on which view is open.
    Confirm,
    /// A printable character typed into the palette or an editing field.
    PaletteChar(char),
    PaletteBackspace,
    Ignored,
}

/// What a raw keypress means depends on which view is open and whether a
/// config-editor field is mid-edit — `read_event` needs this to decide, for
/// example, whether `Char('t')` types the letter "t" into a URL field or
/// toggles the theme picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    /// Dashboard: single-key shortcuts (`q`, `r`, `+`/`-`, `:`, `t`, `c`).
    Normal,
    /// A modal view (theme picker / config editor) is open but no text
    /// field is being edited: arrows navigate, Enter acts, Esc closes.
    ModalNav,
    /// A text field (palette input, or a config-editor field mid-edit) is
    /// capturing every printable character.
    TextEntry,
}

fn input_mode(app: &TuiApp) -> InputMode {
    match app.view {
        View::Dashboard => InputMode::Normal,
        View::CommandPalette => InputMode::TextEntry,
        View::ConfigEditor if app.config_editor_editing.is_some() => InputMode::TextEntry,
        View::ConfigEditor | View::ThemeSettings => InputMode::ModalNav,
    }
}

trait TerminalRuntime {
    fn enter(&mut self) -> Result<(), RuntimeError>;
    fn draw(&mut self, app: &TuiApp) -> Result<(), RuntimeError>;
    async fn poll_event(&mut self, timeout: Duration) -> Result<bool, RuntimeError>;
    async fn read_event(&mut self, mode: InputMode) -> Result<RuntimeEvent, RuntimeError>;
    fn cleanup(&mut self) -> Result<(), RuntimeError>;
}

const SPLASH_FRAME_INTERVAL: Duration = Duration::from_millis(50);

/// Shows the startup splash for a fixed number of frames using the app's
/// already-resolved theme. Any keypress skips it immediately; that key is
/// returned rather than discarded so the caller can feed it into the main
/// loop as a priming event — pressing `q`/`Esc` during the splash both
/// dismisses it and quits in the same keystroke, which is what keeps the
/// single-keypress PTY shutdown tests passing unmodified.
async fn run_splash<T>(
    terminal: &mut T,
    app: &mut TuiApp,
) -> Result<Option<RuntimeEvent>, RuntimeError>
where
    T: TerminalRuntime,
{
    let mut priming_event = None;

    for frame in 0..theme::SPLASH_TOTAL_FRAMES {
        app.splash_frame = Some(frame);
        terminal.draw(app)?;

        if terminal.poll_event(SPLASH_FRAME_INTERVAL).await? {
            priming_event = Some(terminal.read_event(InputMode::Normal).await?);
            break;
        }
    }

    app.splash_frame = None;
    Ok(priming_event)
}

#[derive(Default)]
struct CrosstermRuntime {
    session: Option<TerminalSession>,
}

impl TerminalRuntime for CrosstermRuntime {
    fn enter(&mut self) -> Result<(), RuntimeError> {
        self.session = Some(TerminalSession::enter()?);
        Ok(())
    }

    fn draw(&mut self, app: &TuiApp) -> Result<(), RuntimeError> {
        self.session_mut()?.draw(app)
    }

    async fn poll_event(&mut self, timeout: Duration) -> Result<bool, RuntimeError> {
        blocking_terminal_call(move || event::poll(timeout)).await
    }

    async fn read_event(&mut self, mode: InputMode) -> Result<RuntimeEvent, RuntimeError> {
        let event = blocking_terminal_call(event::read).await?;
        let Event::Key(key) = event else {
            return Ok(RuntimeEvent::Ignored);
        };
        if key.kind != KeyEventKind::Press {
            return Ok(RuntimeEvent::Ignored);
        }

        Ok(match mode {
            InputMode::TextEntry => match key.code {
                KeyCode::Esc => RuntimeEvent::Escape,
                KeyCode::Enter => RuntimeEvent::Confirm,
                KeyCode::Backspace => RuntimeEvent::PaletteBackspace,
                KeyCode::Char(c) => RuntimeEvent::PaletteChar(c),
                _ => RuntimeEvent::Ignored,
            },
            InputMode::ModalNav => match key.code {
                KeyCode::Esc
                | KeyCode::F(2)
                | KeyCode::Char('q')
                | KeyCode::Char('c')
                | KeyCode::Char('C') => RuntimeEvent::Escape,
                KeyCode::Up | KeyCode::Char('k') => RuntimeEvent::NavUp,
                KeyCode::Down | KeyCode::Char('j') => RuntimeEvent::NavDown,
                KeyCode::Enter => RuntimeEvent::Confirm,
                _ => RuntimeEvent::Ignored,
            },
            InputMode::Normal => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => RuntimeEvent::Quit,
                KeyCode::Char('r') | KeyCode::Char('R') => RuntimeEvent::Refresh,
                KeyCode::Char('+') | KeyCode::Char('=') => RuntimeEvent::IncreaseRefreshInterval,
                KeyCode::Char('-') | KeyCode::Char('_') => RuntimeEvent::DecreaseRefreshInterval,
                KeyCode::Char(':') => RuntimeEvent::OpenPalette,
                KeyCode::Char('t') | KeyCode::Char('T') | KeyCode::F(2) => {
                    RuntimeEvent::ToggleThemeSettings
                }
                KeyCode::Char('c') | KeyCode::Char('C') => RuntimeEvent::ToggleConfigEditor,
                _ => RuntimeEvent::Ignored,
            },
        })
    }

    fn cleanup(&mut self) -> Result<(), RuntimeError> {
        match self.session.as_mut() {
            Some(session) => session.restore(),
            None => Ok(()),
        }
    }
}

async fn blocking_terminal_call<T>(
    call: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> Result<T, RuntimeError>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(call)
        .await
        .map_err(|error| io::Error::other(format!("terminal task failed: {error}")))?
        .map_err(RuntimeError::from)
}

impl CrosstermRuntime {
    fn session_mut(&mut self) -> Result<&mut TerminalSession, RuntimeError> {
        self.session
            .as_mut()
            .ok_or_else(|| io::Error::other("terminal session was not initialized").into())
    }
}

#[derive(Debug)]
struct EffectiveConfig {
    configured_url: Option<Url>,
    refresh_interval: Duration,
    theme: Theme,
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

        let theme_id = options.theme_override.as_deref().unwrap_or(&config.theme);

        Self {
            configured_url,
            refresh_interval: Duration::from_secs(refresh_secs),
            theme: Theme::by_id(theme_id),
            current_error,
        }
    }
}

fn runtime_refresh_schedule_interval(
    production_interval: Duration,
    override_value: Option<&str>,
) -> Duration {
    let Some(millis) = override_value.and_then(|value| value.parse::<u64>().ok()) else {
        return production_interval;
    };

    let override_interval = Duration::from_millis(millis);
    let accepted_override = override_interval >= MIN_TUI_TEST_REFRESH_INTERVAL
        && override_interval < production_interval;

    if accepted_override {
        override_interval
    } else {
        production_interval
    }
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
            cleanup_failed_terminal_setup(&mut stdout, raw_mode_enabled, false);
            return Err(error.into());
        }

        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut stdout = io::stdout();
                cleanup_failed_terminal_setup(&mut stdout, raw_mode_enabled, true);
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
        let cleanup_steps =
            terminal_cleanup_steps(self.raw_mode_enabled, self.alternate_screen_enabled);

        for step in cleanup_steps {
            match step {
                TerminalCleanupStep::LeaveAlternateScreen => {
                    if let Err(error) = execute!(self.terminal.backend_mut(), LeaveAlternateScreen)
                    {
                        first_error.get_or_insert(error);
                    } else {
                        self.alternate_screen_enabled = false;
                    }
                }
                TerminalCleanupStep::ShowCursor => {
                    if let Err(error) = self.terminal.show_cursor() {
                        first_error.get_or_insert(error);
                    }
                }
                TerminalCleanupStep::DisableRawMode => {
                    if let Err(error) = disable_raw_mode() {
                        first_error.get_or_insert(error);
                    } else {
                        self.raw_mode_enabled = false;
                    }
                }
            }
        }

        match first_error {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalCleanupStep {
    LeaveAlternateScreen,
    ShowCursor,
    DisableRawMode,
}

fn terminal_cleanup_steps(
    raw_mode_enabled: bool,
    alternate_screen_enabled: bool,
) -> Vec<TerminalCleanupStep> {
    let mut steps = Vec::new();

    if alternate_screen_enabled {
        steps.push(TerminalCleanupStep::LeaveAlternateScreen);
    }

    if raw_mode_enabled || alternate_screen_enabled {
        steps.push(TerminalCleanupStep::ShowCursor);
    }

    if raw_mode_enabled {
        steps.push(TerminalCleanupStep::DisableRawMode);
    }

    steps
}

fn cleanup_failed_terminal_setup<W: io::Write>(
    writer: &mut W,
    raw_mode_enabled: bool,
    alternate_screen_enabled: bool,
) {
    for step in terminal_cleanup_steps(raw_mode_enabled, alternate_screen_enabled) {
        match step {
            TerminalCleanupStep::LeaveAlternateScreen => {
                let _ = execute!(writer, LeaveAlternateScreen);
            }
            TerminalCleanupStep::ShowCursor => {
                let _ = execute!(writer, Show);
            }
            TerminalCleanupStep::DisableRawMode => {
                let _ = disable_raw_mode();
            }
        }
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
    use crate::config::{MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS};
    use serde_json::json;
    use std::{
        cell::Cell,
        collections::VecDeque,
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc::{self, Receiver, Sender},
        },
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum RuntimeCall {
        Enter,
        Draw,
        Poll,
        Read,
        Cleanup,
    }

    #[derive(Debug)]
    struct HarnessTerminal {
        events: VecDeque<RuntimeEvent>,
        polls: VecDeque<Result<bool, io::Error>>,
        clock: Rc<Cell<Instant>>,
        poll_timeouts: Vec<Duration>,
        poll_advances: VecDeque<Duration>,
        read_advances: VecDeque<Duration>,
        draw_error: Option<io::Error>,
        read_error: Option<io::Error>,
        cleanup_error: Option<io::Error>,
        calls: Vec<RuntimeCall>,
        drawn_errors: Vec<Option<String>>,
        cleanup_called: bool,
    }

    impl HarnessTerminal {
        fn with_events(events: impl IntoIterator<Item = RuntimeEvent>) -> Self {
            Self {
                events: events.into_iter().collect(),
                polls: VecDeque::new(),
                clock: test_clock_start(),
                poll_timeouts: Vec::new(),
                poll_advances: VecDeque::new(),
                read_advances: VecDeque::new(),
                draw_error: None,
                read_error: None,
                cleanup_error: None,
                calls: Vec::new(),
                drawn_errors: Vec::new(),
                cleanup_called: false,
            }
        }

        fn with_quit() -> Self {
            Self::with_events([RuntimeEvent::Quit])
        }

        fn fail_draw(mut self, message: &'static str) -> Self {
            self.draw_error = Some(io::Error::other(message));
            self
        }

        fn fail_poll(mut self, message: &'static str) -> Self {
            self.polls.push_back(Err(io::Error::other(message)));
            self
        }

        fn fail_read(mut self, message: &'static str) -> Self {
            self.read_error = Some(io::Error::other(message));
            self
        }

        fn fail_cleanup(mut self, message: &'static str) -> Self {
            self.cleanup_error = Some(io::Error::other(message));
            self
        }

        fn with_clock(mut self, clock: Rc<Cell<Instant>>) -> Self {
            self.clock = clock;
            self
        }

        fn poll_advance(mut self, duration: Duration) -> Self {
            self.poll_advances.push_back(duration);
            self
        }

        fn read_advance(mut self, duration: Duration) -> Self {
            self.read_advances.push_back(duration);
            self
        }

        fn advance_clock(&self, duration: Duration) {
            self.clock.set(self.clock.get() + duration);
        }
    }

    impl TerminalRuntime for HarnessTerminal {
        fn enter(&mut self) -> Result<(), RuntimeError> {
            self.calls.push(RuntimeCall::Enter);
            Ok(())
        }

        fn draw(&mut self, app: &TuiApp) -> Result<(), RuntimeError> {
            self.calls.push(RuntimeCall::Draw);
            self.drawn_errors.push(app.current_error.clone());
            if let Some(error) = self.draw_error.take() {
                return Err(error.into());
            }
            Ok(())
        }

        async fn poll_event(&mut self, timeout: Duration) -> Result<bool, RuntimeError> {
            self.calls.push(RuntimeCall::Poll);
            self.poll_timeouts.push(timeout);
            if let Some(duration) = self.poll_advances.pop_front() {
                self.advance_clock(duration);
            }
            if let Some(result) = self.polls.pop_front() {
                return result.map_err(RuntimeError::from);
            }
            Ok(!self.events.is_empty())
        }

        async fn read_event(&mut self, _mode: InputMode) -> Result<RuntimeEvent, RuntimeError> {
            self.calls.push(RuntimeCall::Read);
            if let Some(duration) = self.read_advances.pop_front() {
                self.advance_clock(duration);
            }
            if let Some(error) = self.read_error.take() {
                return Err(error.into());
            }
            Ok(self.events.pop_front().unwrap_or(RuntimeEvent::Ignored))
        }

        fn cleanup(&mut self) -> Result<(), RuntimeError> {
            self.calls.push(RuntimeCall::Cleanup);
            self.cleanup_called = true;
            match self.cleanup_error.take() {
                Some(error) => Err(error.into()),
                None => Ok(()),
            }
        }
    }

    #[derive(Debug)]
    struct FakeClock {
        now: Rc<Cell<Instant>>,
    }

    impl FakeClock {
        fn new(now: Rc<Cell<Instant>>) -> Self {
            Self { now }
        }
    }

    impl RuntimeClock for FakeClock {
        fn now(&mut self) -> Instant {
            self.now.get()
        }
    }

    fn test_clock_start() -> Rc<Cell<Instant>> {
        Rc::new(Cell::new(Instant::now()))
    }

    async fn run_with_harness_clock<T, F>(
        terminal: &mut T,
        app: &mut TuiApp,
        fetcher: &mut F,
        clock: Rc<Cell<Instant>>,
    ) -> Result<(), RuntimeError>
    where
        T: TerminalRuntime,
        F: MeasureFetchWorker,
    {
        terminal.enter()?;

        let mut clock = FakeClock::new(clock);
        let refresh_schedule_interval = app.refresh_interval;
        let result = run_loop(
            terminal,
            app,
            fetcher,
            &mut clock,
            refresh_schedule_interval,
            None,
        )
        .await;
        let cleanup_result = terminal.cleanup();

        match (result, cleanup_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(primary), Ok(())) => Err(primary),
            (Ok(()), Err(cleanup)) => Err(cleanup),
            (Err(primary), Err(cleanup)) => {
                Err(RuntimeError::with_cleanup_failure(primary, cleanup))
            }
        }
    }

    #[derive(Debug)]
    struct HarnessFetcher {
        completions: VecDeque<Option<Result<Value, String>>>,
        stale_completions_after_cancel: VecDeque<Result<Value, String>>,
        calls: Vec<Url>,
        active_fetch: bool,
        canceled_fetches: usize,
        cancel_error: Option<&'static str>,
    }

    impl HarnessFetcher {
        fn new(results: impl IntoIterator<Item = Result<Value, String>>) -> Self {
            Self {
                completions: results.into_iter().map(Some).collect(),
                stale_completions_after_cancel: VecDeque::new(),
                calls: Vec::new(),
                active_fetch: false,
                canceled_fetches: 0,
                cancel_error: None,
            }
        }

        fn pending_then(results: impl IntoIterator<Item = Option<Result<Value, String>>>) -> Self {
            Self {
                completions: results.into_iter().collect(),
                stale_completions_after_cancel: VecDeque::new(),
                calls: Vec::new(),
                active_fetch: false,
                canceled_fetches: 0,
                cancel_error: None,
            }
        }

        fn with_stale_after_cancel(
            mut self,
            results: impl IntoIterator<Item = Result<Value, String>>,
        ) -> Self {
            self.stale_completions_after_cancel = results.into_iter().collect();
            self
        }

        fn fail_cancel(mut self, message: &'static str) -> Self {
            self.cancel_error = Some(message);
            self
        }
    }

    impl MeasureFetchWorker for HarnessFetcher {
        fn start_fetch(&mut self, base_url: Url) {
            assert!(
                !self.active_fetch,
                "scheduler started overlapping harness fetches"
            );
            self.active_fetch = true;
            self.calls.push(base_url);
        }

        async fn recv_ready_fetch(&mut self) -> Result<Option<FetchCompletion>, RuntimeError> {
            if !self.active_fetch {
                return Ok(self
                    .stale_completions_after_cancel
                    .pop_front()
                    .map(|result| FetchCompletion {
                        result,
                        duration: Duration::from_millis(25),
                        completed_at: SystemTime::UNIX_EPOCH + Duration::from_secs(60),
                    }));
            }

            let Some(result) = self.completions.pop_front().flatten() else {
                return Ok(None);
            };
            self.active_fetch = false;
            Ok(Some(FetchCompletion {
                result,
                duration: Duration::from_millis(25),
                completed_at: SystemTime::UNIX_EPOCH + Duration::from_secs(60),
            }))
        }

        async fn cancel_fetch(&mut self) -> Result<(), RuntimeError> {
            if self.active_fetch {
                self.canceled_fetches += 1;
                self.active_fetch = false;
            }
            if let Some(message) = self.cancel_error.take() {
                return Err(RuntimeError::FetchTask(message.to_owned()));
            }
            Ok(())
        }
    }

    #[derive(Debug)]
    struct BlockingPollTerminal {
        poll_delay: Duration,
        cleanup_called: bool,
    }

    impl BlockingPollTerminal {
        fn new(poll_delay: Duration) -> Self {
            Self {
                poll_delay,
                cleanup_called: false,
            }
        }
    }

    impl TerminalRuntime for BlockingPollTerminal {
        fn enter(&mut self) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn draw(&mut self, _app: &TuiApp) -> Result<(), RuntimeError> {
            Ok(())
        }

        async fn poll_event(&mut self, _timeout: Duration) -> Result<bool, RuntimeError> {
            let poll_delay = self.poll_delay;
            blocking_terminal_call(move || {
                std::thread::sleep(poll_delay);
                Ok(true)
            })
            .await
        }

        async fn read_event(&mut self, _mode: InputMode) -> Result<RuntimeEvent, RuntimeError> {
            Ok(RuntimeEvent::Quit)
        }

        fn cleanup(&mut self) -> Result<(), RuntimeError> {
            self.cleanup_called = true;
            Ok(())
        }
    }

    #[derive(Debug)]
    struct YieldingFetcher {
        sender: Sender<FetchCompletion>,
        receiver: Receiver<FetchCompletion>,
        completed: Arc<AtomicBool>,
        active_handle: Option<JoinHandle<()>>,
        calls: Vec<Url>,
    }

    impl YieldingFetcher {
        fn new(completed: Arc<AtomicBool>) -> Self {
            let (sender, receiver) = mpsc::channel();
            Self {
                sender,
                receiver,
                completed,
                active_handle: None,
                calls: Vec::new(),
            }
        }
    }

    impl MeasureFetchWorker for YieldingFetcher {
        fn start_fetch(&mut self, base_url: Url) {
            assert!(
                self.active_handle.is_none(),
                "scheduler started overlapping yielding fetches"
            );
            self.calls.push(base_url);

            let sender = self.sender.clone();
            let completed = self.completed.clone();
            self.active_handle = Some(tokio::spawn(async move {
                tokio::task::yield_now().await;
                completed.store(true, Ordering::SeqCst);
                let _ = sender.send(FetchCompletion {
                    result: Ok(successful_payload()),
                    duration: Duration::from_millis(1),
                    completed_at: SystemTime::UNIX_EPOCH + Duration::from_secs(60),
                });
            }));
        }

        async fn recv_ready_fetch(&mut self) -> Result<Option<FetchCompletion>, RuntimeError> {
            Ok(self.receiver.try_recv().ok())
        }

        async fn cancel_fetch(&mut self) -> Result<(), RuntimeError> {
            if let Some(handle) = self.active_handle.take() {
                handle.abort();
                match handle.await {
                    Ok(()) => {}
                    Err(error) if error.is_cancelled() => {}
                    Err(error) => return Err(RuntimeError::FetchTask(error.to_string())),
                }
            }
            while self.receiver.try_recv().is_ok() {}
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct SpawnedPendingFetcher {
        active_handle: Option<JoinHandle<()>>,
        calls: Vec<Url>,
        cancellation_observed: bool,
    }

    impl MeasureFetchWorker for SpawnedPendingFetcher {
        fn start_fetch(&mut self, base_url: Url) {
            assert!(
                self.active_handle.is_none(),
                "scheduler started overlapping pending fetches"
            );
            self.calls.push(base_url);
            self.active_handle = Some(tokio::spawn(async {
                std::future::pending::<()>().await;
            }));
        }

        async fn recv_ready_fetch(&mut self) -> Result<Option<FetchCompletion>, RuntimeError> {
            Ok(None)
        }

        async fn cancel_fetch(&mut self) -> Result<(), RuntimeError> {
            if let Some(handle) = self.active_handle.take() {
                handle.abort();
                match handle.await {
                    Ok(()) => {}
                    Err(error) if error.is_cancelled() => {
                        self.cancellation_observed = true;
                    }
                    Err(error) => return Err(RuntimeError::FetchTask(error.to_string())),
                }
            }
            Ok(())
        }
    }

    fn app(configured_url: Option<Url>) -> TuiApp {
        TuiApp::new(configured_url, Duration::from_secs(30))
    }

    fn configured_url() -> Url {
        Url::parse("http://192.168.1.201/").expect("test URL should parse")
    }

    #[test]
    fn production_refresh_clamp_keeps_documented_bounds() {
        assert_eq!(
            TuiApp::new(None, Duration::from_secs(0)).refresh_interval,
            Duration::from_secs(MIN_REFRESH_INTERVAL_SECS)
        );
        assert_eq!(
            TuiApp::new(None, Duration::from_secs(1)).refresh_interval,
            Duration::from_secs(MIN_REFRESH_INTERVAL_SECS)
        );
        assert_eq!(
            TuiApp::new(None, Duration::from_secs(4)).refresh_interval,
            Duration::from_secs(MIN_REFRESH_INTERVAL_SECS)
        );
        assert_eq!(
            TuiApp::new(None, Duration::from_secs(5)).refresh_interval,
            Duration::from_secs(MIN_REFRESH_INTERVAL_SECS)
        );
        assert_eq!(
            TuiApp::new(None, Duration::from_secs(3601)).refresh_interval,
            Duration::from_secs(MAX_REFRESH_INTERVAL_SECS)
        );
    }

    fn assert_runtime_schedule_override(
        production_seconds: u64,
        override_value: Option<&str>,
        expected_schedule_interval: Duration,
    ) {
        let app = TuiApp::new(None, Duration::from_secs(production_seconds));
        let production_interval = app.refresh_interval;

        assert_eq!(
            runtime_refresh_schedule_interval(production_interval, override_value),
            expected_schedule_interval
        );
        assert_eq!(app.refresh_interval, production_interval);
    }

    #[test]
    fn runtime_refresh_schedule_override_can_shorten_without_changing_app_interval() {
        assert_runtime_schedule_override(
            MIN_REFRESH_INTERVAL_SECS,
            Some("250"),
            Duration::from_millis(250),
        );
    }

    #[test]
    fn minimum_runtime_refresh_schedule_override_can_shorten_without_changing_app_interval() {
        assert_runtime_schedule_override(
            MIN_REFRESH_INTERVAL_SECS,
            Some("100"),
            Duration::from_millis(100),
        );
    }

    #[test]
    fn invalid_runtime_refresh_schedule_override_keeps_production_interval() {
        let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

        assert_runtime_schedule_override(
            MIN_REFRESH_INTERVAL_SECS,
            Some("invalid"),
            production_interval,
        );
    }

    #[test]
    fn below_minimum_runtime_refresh_schedule_override_keeps_production_interval() {
        let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

        assert_runtime_schedule_override(
            MIN_REFRESH_INTERVAL_SECS,
            Some("99"),
            production_interval,
        );
    }

    #[test]
    fn zero_runtime_refresh_schedule_override_keeps_production_interval() {
        let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

        assert_runtime_schedule_override(MIN_REFRESH_INTERVAL_SECS, Some("0"), production_interval);
    }

    #[test]
    fn equal_runtime_refresh_schedule_override_keeps_production_interval() {
        let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

        assert_runtime_schedule_override(
            MIN_REFRESH_INTERVAL_SECS,
            Some("5000"),
            production_interval,
        );
    }

    #[test]
    fn longer_runtime_refresh_schedule_override_keeps_production_interval() {
        let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

        assert_runtime_schedule_override(
            MIN_REFRESH_INTERVAL_SECS,
            Some("6000"),
            production_interval,
        );
    }

    #[test]
    fn missing_runtime_refresh_schedule_override_keeps_production_interval() {
        let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

        assert_runtime_schedule_override(MIN_REFRESH_INTERVAL_SECS, None, production_interval);
    }

    fn successful_payload() -> Value {
        json!({
            "aqi": 42,
            "rco2": 612,
            "pm02": 7.4
        })
    }

    fn later_successful_payload() -> Value {
        json!({
            "aqi": 55,
            "rco2": 650,
            "pm02": 8.1
        })
    }

    fn terminal_error_message(error: &RuntimeError) -> String {
        match error {
            RuntimeError::Terminal(error) => error.to_string(),
            RuntimeError::Device(error) => error.to_string(),
            RuntimeError::NonInteractiveTerminal => error.to_string(),
            RuntimeError::FetchTask(message) => message.clone(),
            RuntimeError::Secondary { primary, .. } => terminal_error_message(primary),
            RuntimeError::Cleanup { primary, .. } => terminal_error_message(primary),
        }
    }

    fn secondary_error_message(error: &RuntimeError) -> Option<String> {
        match error {
            RuntimeError::Secondary { secondary, .. } => Some(secondary.to_string()),
            RuntimeError::Cleanup { primary, .. } => secondary_error_message(primary),
            _ => None,
        }
    }

    fn cleanup_error_message(error: &RuntimeError) -> Option<String> {
        match error {
            RuntimeError::Cleanup { cleanup, .. } => Some(cleanup.to_string()),
            _ => None,
        }
    }

    #[tokio::test]
    async fn run_splash_draws_every_frame_and_returns_none_when_no_key_arrives() {
        let mut terminal = HarnessTerminal::with_events([]);
        let mut app = app(None);

        let priming = run_splash(&mut terminal, &mut app)
            .await
            .expect("splash should not error without a terminal failure");

        assert_eq!(priming, None);
        assert_eq!(app.splash_frame, None);
        let draw_calls = terminal
            .calls
            .iter()
            .filter(|call| **call == RuntimeCall::Draw)
            .count();
        assert_eq!(draw_calls as u64, theme::SPLASH_TOTAL_FRAMES);
    }

    #[tokio::test]
    async fn run_splash_skips_immediately_and_returns_the_triggering_key() {
        let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]);
        let mut app = app(None);

        let priming = run_splash(&mut terminal, &mut app)
            .await
            .expect("splash should not error without a terminal failure");

        assert_eq!(priming, Some(RuntimeEvent::Quit));
        assert_eq!(app.splash_frame, None);
        let draw_calls = terminal
            .calls
            .iter()
            .filter(|call| **call == RuntimeCall::Draw)
            .count();
        assert_eq!(draw_calls, 1);
    }

    #[tokio::test]
    async fn splash_priming_event_is_honored_as_first_loop_event() {
        // A key pressed during the splash should both skip it and take
        // effect immediately in the main loop — e.g. `q` both dismisses the
        // splash and quits in the same keystroke, rather than requiring a
        // second press. The priming event must not be silently discarded.
        let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]);
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);
        let refresh_interval = app.refresh_interval;

        run_with_adapters_with_refresh_interval(
            &mut terminal,
            &mut app,
            &mut fetcher,
            refresh_interval,
            true,
        )
        .await
        .expect("runtime should quit cleanly via the priming event");

        assert_eq!(app.splash_frame, None);
        // One splash-frame draw, then run_loop's own pre-poll draw; the
        // queued Quit event is consumed once by the splash and never
        // reaches a second poll/read cycle.
        let draw_calls = terminal
            .calls
            .iter()
            .filter(|call| **call == RuntimeCall::Draw)
            .count();
        assert_eq!(draw_calls, 2);
    }

    #[tokio::test]
    async fn harness_drives_normal_quit_without_fetch() {
        let mut terminal = HarnessTerminal::with_quit();
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(
            terminal.calls,
            [
                RuntimeCall::Enter,
                RuntimeCall::Draw,
                RuntimeCall::Poll,
                RuntimeCall::Read,
                RuntimeCall::Cleanup,
            ]
        );
        assert!(terminal.cleanup_called);
        assert!(fetcher.calls.is_empty());
    }

    #[tokio::test]
    async fn harness_records_fetch_success_before_first_draw() {
        let mut terminal = HarnessTerminal::with_quit();
        let mut fetcher = HarnessFetcher::new([Ok(successful_payload())]);
        let url = configured_url();
        let mut app = app(Some(url.clone()));

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls, [url]);
        assert_eq!(
            app.current_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.aqi),
            Some(42.0)
        );
        assert_eq!(app.current_error, None);
        assert_eq!(terminal.drawn_errors, [None]);
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn harness_records_fetch_failure_before_first_draw() {
        let mut terminal = HarnessTerminal::with_quit();
        let mut fetcher = HarnessFetcher::new([Err("request timed out".to_owned())]);
        let mut app = app(Some(configured_url()));

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(app.current_snapshot, None);
        assert_eq!(app.current_error.as_deref(), Some("request timed out"));
        assert_eq!(
            terminal.drawn_errors,
            [Some("request timed out".to_owned())]
        );
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn initial_fetch_is_started_without_blocking_quit() {
        let mut terminal = HarnessTerminal::with_quit();
        let mut fetcher = HarnessFetcher::pending_then([None]);
        let url = configured_url();
        let mut app = app(Some(url.clone()));

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls, [url]);
        assert_eq!(app.current_snapshot, None);
        assert_eq!(
            terminal.calls,
            [
                RuntimeCall::Enter,
                RuntimeCall::Draw,
                RuntimeCall::Poll,
                RuntimeCall::Read,
                RuntimeCall::Cleanup,
            ]
        );
    }

    #[tokio::test]
    async fn interval_refresh_starts_after_refresh_deadline() {
        let clock = test_clock_start();
        let mut terminal =
            HarnessTerminal::with_events([RuntimeEvent::Quit]).with_clock(clock.clone());
        for _ in 0..50 {
            terminal.polls.push_back(Ok(false));
            terminal.poll_advances.push_back(Duration::from_millis(100));
        }
        let mut fetcher =
            HarnessFetcher::new([Ok(successful_payload()), Ok(later_successful_payload())]);
        let url = configured_url();
        let mut app = app(Some(url.clone()));
        app.refresh_interval = Duration::from_secs(5);

        run_with_harness_clock(&mut terminal, &mut app, &mut fetcher, clock)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls, [url.clone(), url]);
        assert_eq!(
            app.current_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.aqi),
            Some(55.0)
        );
        assert_eq!(
            app.previous_successful_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.aqi),
            Some(42.0)
        );
    }

    #[tokio::test]
    async fn runtime_refresh_schedule_can_shorten_without_mutating_app_interval() {
        let clock = test_clock_start();
        let mut terminal =
            HarnessTerminal::with_events([RuntimeEvent::Quit]).with_clock(clock.clone());
        for _ in 0..3 {
            terminal.polls.push_back(Ok(false));
            terminal.poll_advances.push_back(Duration::from_millis(100));
        }
        let mut fetcher =
            HarnessFetcher::new([Ok(successful_payload()), Ok(later_successful_payload())]);
        let url = configured_url();
        let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);
        let mut app = TuiApp::new(Some(url.clone()), production_interval);
        let refresh_schedule_interval =
            runtime_refresh_schedule_interval(production_interval, Some("250"));

        terminal.enter().expect("terminal should enter");
        let mut fake_clock = FakeClock::new(clock);
        let result = run_loop(
            &mut terminal,
            &mut app,
            &mut fetcher,
            &mut fake_clock,
            refresh_schedule_interval,
            None,
        )
        .await;
        let cleanup_result = terminal.cleanup();

        result.expect("runtime should quit cleanly");
        cleanup_result.expect("cleanup should succeed");
        assert_eq!(refresh_schedule_interval, Duration::from_millis(250));
        assert_eq!(app.refresh_interval, production_interval);
        assert_eq!(fetcher.calls, [url.clone(), url]);
        assert_eq!(
            app.current_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.aqi),
            Some(55.0)
        );
    }

    #[tokio::test]
    async fn early_false_polls_do_not_fire_interval_refresh_early() {
        let clock = test_clock_start();
        let mut terminal =
            HarnessTerminal::with_events([RuntimeEvent::Quit]).with_clock(clock.clone());
        for _ in 0..50 {
            terminal.polls.push_back(Ok(false));
            terminal.poll_advances.push_back(Duration::ZERO);
        }
        let url = configured_url();
        let mut fetcher = HarnessFetcher::new([Ok(successful_payload())]);
        let mut app = app(Some(url.clone()));
        app.refresh_interval = Duration::from_secs(5);

        run_with_harness_clock(&mut terminal, &mut app, &mut fetcher, clock)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls, [url]);
        assert!(
            terminal
                .poll_timeouts
                .iter()
                .all(|timeout| *timeout <= FETCH_RESULT_POLL_INTERVAL)
        );
    }

    #[tokio::test]
    async fn delayed_false_poll_handles_at_most_one_interval_deadline() {
        let clock = test_clock_start();
        let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit])
            .with_clock(clock.clone())
            .poll_advance(Duration::from_secs(16));
        terminal.polls.push_back(Ok(false));
        let url = configured_url();
        let mut fetcher =
            HarnessFetcher::new([Ok(successful_payload()), Ok(later_successful_payload())]);
        let mut app = app(Some(url.clone()));
        app.refresh_interval = Duration::from_secs(5);

        run_with_harness_clock(&mut terminal, &mut app, &mut fetcher, clock)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls, [url.clone(), url]);
        assert_eq!(
            app.current_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.aqi),
            Some(55.0)
        );
    }

    #[tokio::test]
    async fn manual_refresh_resets_interval_from_event_read_time() {
        let clock = test_clock_start();
        let mut terminal =
            HarnessTerminal::with_events([RuntimeEvent::Refresh, RuntimeEvent::Quit])
                .with_clock(clock.clone())
                .poll_advance(Duration::from_secs(2))
                .read_advance(Duration::from_secs(3))
                .poll_advance(Duration::from_secs(4));
        terminal.polls.push_back(Ok(true));
        terminal.polls.push_back(Ok(false));
        terminal.polls.push_back(Ok(true));
        let url = configured_url();
        let mut fetcher =
            HarnessFetcher::new([Ok(successful_payload()), Ok(later_successful_payload())]);
        let mut app = app(Some(url.clone()));
        app.refresh_interval = Duration::from_secs(5);

        run_with_harness_clock(&mut terminal, &mut app, &mut fetcher, clock)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls, [url.clone(), url]);
        assert_eq!(
            app.current_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.aqi),
            Some(55.0)
        );
    }

    #[tokio::test]
    async fn manual_refresh_during_in_flight_fetch_is_coalesced() {
        let mut terminal =
            HarnessTerminal::with_events([RuntimeEvent::Refresh, RuntimeEvent::Quit]);
        let mut fetcher = HarnessFetcher::pending_then([
            None,
            None,
            Some(Ok(successful_payload())),
            Some(Ok(later_successful_payload())),
        ]);
        let url = configured_url();
        let mut app = app(Some(url.clone()));

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls, [url.clone(), url]);
        assert_eq!(
            app.current_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.aqi),
            Some(55.0)
        );
        assert_eq!(
            app.previous_successful_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.aqi),
            Some(42.0)
        );
    }

    #[tokio::test]
    async fn refresh_interval_key_events_adjust_app_interval() {
        let mut terminal = HarnessTerminal::with_events([
            RuntimeEvent::IncreaseRefreshInterval,
            RuntimeEvent::IncreaseRefreshInterval,
            RuntimeEvent::DecreaseRefreshInterval,
            RuntimeEvent::Quit,
        ]);
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(app.refresh_interval, Duration::from_secs(35));
        assert!(fetcher.calls.is_empty());
        assert_eq!(
            terminal
                .calls
                .iter()
                .filter(|call| **call == RuntimeCall::Draw)
                .count(),
            4
        );
    }

    #[tokio::test]
    async fn failure_after_success_preserves_last_successful_snapshot() {
        let mut terminal =
            HarnessTerminal::with_events([RuntimeEvent::Refresh, RuntimeEvent::Quit]);
        let mut fetcher =
            HarnessFetcher::new([Ok(successful_payload()), Err("refresh failed".to_owned())]);
        let url = configured_url();
        let mut app = app(Some(url.clone()));

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls, [url.clone(), url]);
        assert_eq!(
            app.current_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.aqi),
            Some(42.0)
        );
        assert_eq!(app.current_error.as_deref(), Some("refresh failed"));
    }

    #[tokio::test]
    async fn quit_while_fetch_is_pending_requests_cancellation() {
        let mut terminal = HarnessTerminal::with_quit();
        let mut fetcher = HarnessFetcher::pending_then([None]);
        let mut app = app(Some(configured_url()));

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls.len(), 1);
        assert_eq!(fetcher.canceled_fetches, 1);
        assert_eq!(terminal.calls.last(), Some(&RuntimeCall::Cleanup));
        assert_eq!(app.current_snapshot, None);
        assert!(!app.is_fetching);
    }

    #[tokio::test]
    async fn quit_while_spawned_fetch_is_pending_observes_cancellation() {
        let mut terminal = HarnessTerminal::with_quit();
        let mut fetcher = SpawnedPendingFetcher::default();
        let mut app = app(Some(configured_url()));

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls.len(), 1);
        assert!(fetcher.active_handle.is_none());
        assert!(fetcher.cancellation_observed);
        assert!(!app.is_fetching);
        assert!(terminal.cleanup_called);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn current_thread_runtime_progresses_fetch_while_terminal_poll_blocks() {
        let fetch_completed = Arc::new(AtomicBool::new(false));
        let mut terminal = BlockingPollTerminal::new(Duration::from_millis(25));
        let mut fetcher = YieldingFetcher::new(fetch_completed.clone());
        let mut app = app(Some(configured_url()));

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect("runtime should quit cleanly");

        assert!(terminal.cleanup_called);
        assert_eq!(fetcher.calls.len(), 1);
        assert!(
            fetch_completed.load(Ordering::SeqCst),
            "background fetch task should progress while terminal polling is blocking"
        );
    }

    #[tokio::test]
    async fn draw_failure_cancels_pending_fetch() {
        let mut terminal = HarnessTerminal::with_quit().fail_draw("draw failed");
        let mut fetcher = HarnessFetcher::pending_then([None]);
        let mut app = app(Some(configured_url()));

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("draw failure should be returned");

        assert_eq!(terminal_error_message(&error), "draw failed");
        assert_eq!(fetcher.calls.len(), 1);
        assert_eq!(fetcher.canceled_fetches, 1);
        assert_eq!(app.current_snapshot, None);
        assert!(!app.is_fetching);
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn draw_failure_retains_panicked_fetch_context_from_cancellation() {
        let mut terminal = HarnessTerminal::with_quit().fail_draw("draw failed");
        let mut fetcher = HarnessFetcher::pending_then([None]).fail_cancel("fetch task exploded");
        let mut app = app(Some(configured_url()));

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("draw failure should retain fetch cancellation context");

        assert_eq!(terminal_error_message(&error), "draw failed");
        assert_eq!(
            secondary_error_message(&error).as_deref(),
            Some("background fetch task failed: fetch task exploded")
        );
        assert_eq!(fetcher.canceled_fetches, 1);
        assert!(!app.is_fetching);
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn poll_failure_cancels_pending_fetch() {
        let mut terminal = HarnessTerminal::with_events([]).fail_poll("poll failed");
        let mut fetcher = HarnessFetcher::pending_then([None]);
        let mut app = app(Some(configured_url()));

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("poll failure should be returned");

        assert_eq!(terminal_error_message(&error), "poll failed");
        assert_eq!(fetcher.calls.len(), 1);
        assert_eq!(fetcher.canceled_fetches, 1);
        assert_eq!(app.current_snapshot, None);
        assert!(!app.is_fetching);
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn poll_failure_retains_fetch_cancellation_failure_context() {
        let mut terminal = HarnessTerminal::with_events([]).fail_poll("poll failed");
        let mut fetcher =
            HarnessFetcher::pending_then([None]).fail_cancel("fetch cancellation failed");
        let mut app = app(Some(configured_url()));

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("poll failure should retain fetch cancellation context");

        assert_eq!(terminal_error_message(&error), "poll failed");
        assert_eq!(
            secondary_error_message(&error).as_deref(),
            Some("background fetch task failed: fetch cancellation failed")
        );
        assert_eq!(fetcher.canceled_fetches, 1);
        assert!(!app.is_fetching);
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn read_failure_cancels_pending_fetch() {
        let mut terminal =
            HarnessTerminal::with_events([RuntimeEvent::Quit]).fail_read("read failed");
        let mut fetcher = HarnessFetcher::pending_then([None]);
        let mut app = app(Some(configured_url()));

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("read failure should be returned");

        assert_eq!(terminal_error_message(&error), "read failed");
        assert_eq!(fetcher.calls.len(), 1);
        assert_eq!(fetcher.canceled_fetches, 1);
        assert_eq!(app.current_snapshot, None);
        assert!(!app.is_fetching);
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn read_failure_retains_fetch_cancellation_failure_context() {
        let mut terminal =
            HarnessTerminal::with_events([RuntimeEvent::Quit]).fail_read("read failed");
        let mut fetcher =
            HarnessFetcher::pending_then([None]).fail_cancel("fetch cancellation failed");
        let mut app = app(Some(configured_url()));

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("read failure should retain fetch cancellation context");

        assert_eq!(terminal_error_message(&error), "read failed");
        assert_eq!(
            secondary_error_message(&error).as_deref(),
            Some("background fetch task failed: fetch cancellation failed")
        );
        assert_eq!(fetcher.canceled_fetches, 1);
        assert!(!app.is_fetching);
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn stale_completion_after_cancellation_does_not_mutate_app() {
        let url = configured_url();
        let mut app = app(Some(url.clone()));
        let mut fetcher = HarnessFetcher::pending_then([None])
            .with_stale_after_cancel([Ok(successful_payload())]);
        let mut scheduler = FetchScheduler::default();

        scheduler.request_refresh(&mut app, &mut fetcher);
        scheduler
            .cancel_pending_fetch(&mut app, &mut fetcher)
            .await
            .expect("cancellation should be observed");

        assert!(
            !scheduler
                .apply_ready_results(&mut app, &mut fetcher)
                .await
                .expect("stale completion drain should not fail")
        );
        assert_eq!(fetcher.calls, [url]);
        assert_eq!(fetcher.canceled_fetches, 1);
        assert_eq!(app.current_snapshot, None);
        assert_eq!(app.current_error, None);
        assert!(!app.is_fetching);
    }

    #[tokio::test]
    async fn panicked_fetch_task_is_returned_as_runtime_error_when_observed() {
        let handle = tokio::spawn(async {
            panic!("fetch task exploded");
        });

        let error = observe_fetch_handle(handle)
            .await
            .expect_err("panicked fetch task should be surfaced");

        assert!(terminal_error_message(&error).contains("fetch task exploded"));
    }

    #[tokio::test]
    async fn cleanup_runs_after_draw_failure() {
        let mut terminal = HarnessTerminal::with_quit().fail_draw("draw failed");
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("draw failure should be returned");

        assert_eq!(terminal_error_message(&error), "draw failed");
        assert_eq!(
            terminal.calls,
            [RuntimeCall::Enter, RuntimeCall::Draw, RuntimeCall::Cleanup]
        );
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn cleanup_failure_context_is_retained_after_draw_failure() {
        let mut terminal = HarnessTerminal::with_quit()
            .fail_draw("draw failed")
            .fail_cleanup("cleanup failed");
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("draw failure should be returned with cleanup context");

        assert_eq!(terminal_error_message(&error), "draw failed");
        assert_eq!(
            cleanup_error_message(&error).as_deref(),
            Some("cleanup failed")
        );
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn cleanup_runs_after_poll_failure() {
        let mut terminal = HarnessTerminal::with_events([]).fail_poll("poll failed");
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("poll failure should be returned");

        assert_eq!(terminal_error_message(&error), "poll failed");
        assert_eq!(
            terminal.calls,
            [
                RuntimeCall::Enter,
                RuntimeCall::Draw,
                RuntimeCall::Poll,
                RuntimeCall::Cleanup,
            ]
        );
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn cleanup_failure_context_is_retained_after_poll_failure() {
        let mut terminal = HarnessTerminal::with_events([])
            .fail_poll("poll failed")
            .fail_cleanup("cleanup failed");
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("poll failure should be returned with cleanup context");

        assert_eq!(terminal_error_message(&error), "poll failed");
        assert_eq!(
            cleanup_error_message(&error).as_deref(),
            Some("cleanup failed")
        );
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn cleanup_runs_after_read_failure() {
        let mut terminal =
            HarnessTerminal::with_events([RuntimeEvent::Quit]).fail_read("read failed");
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("read failure should be returned");

        assert_eq!(terminal_error_message(&error), "read failed");
        assert_eq!(
            terminal.calls,
            [
                RuntimeCall::Enter,
                RuntimeCall::Draw,
                RuntimeCall::Poll,
                RuntimeCall::Read,
                RuntimeCall::Cleanup,
            ]
        );
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn cleanup_failure_context_is_retained_after_read_failure() {
        let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit])
            .fail_read("read failed")
            .fail_cleanup("cleanup failed");
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("read failure should be returned with cleanup context");

        assert_eq!(terminal_error_message(&error), "read failed");
        assert_eq!(
            cleanup_error_message(&error).as_deref(),
            Some("cleanup failed")
        );
        assert!(terminal.cleanup_called);
    }

    #[tokio::test]
    async fn cleanup_failure_is_returned_after_clean_loop() {
        let mut terminal = HarnessTerminal::with_quit().fail_cleanup("cleanup failed");
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .await
            .expect_err("cleanup failure should be returned");

        assert_eq!(terminal_error_message(&error), "cleanup failed");
        assert!(terminal.cleanup_called);
    }

    #[test]
    fn cleanup_order_restores_screen_and_cursor_before_disabling_raw_mode() {
        assert_eq!(
            terminal_cleanup_steps(true, true),
            vec![
                TerminalCleanupStep::LeaveAlternateScreen,
                TerminalCleanupStep::ShowCursor,
                TerminalCleanupStep::DisableRawMode,
            ]
        );
    }

    #[test]
    fn cleanup_order_handles_partial_setup_after_raw_mode_started() {
        assert_eq!(
            terminal_cleanup_steps(true, false),
            vec![
                TerminalCleanupStep::ShowCursor,
                TerminalCleanupStep::DisableRawMode,
            ]
        );
        assert_eq!(terminal_cleanup_steps(false, false), vec![]);
    }
}
