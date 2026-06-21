use std::{
    io::{self, IsTerminal, Stdout},
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
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
use url::Url;

use crate::{
    config::{self, MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS},
    device::{self, FetchSettings},
    sensors,
    tui::app::TuiApp,
};

const FETCH_RESULT_POLL_INTERVAL: Duration = Duration::from_millis(100);

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

    #[error("TUI requires an interactive terminal")]
    NonInteractiveTerminal,

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
    let mut app = TuiApp::with_client(
        effective_config.configured_url,
        effective_config.refresh_interval,
        options.fetch_settings,
        fetch_client.clone(),
    );

    if let Some(error) = effective_config.current_error {
        app.current_error = Some(error);
    }

    let mut terminal = CrosstermRuntime::default();
    let mut fetcher = BackgroundMeasureFetcher::new(fetch_client);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
}

fn ensure_interactive_terminal() -> Result<(), RuntimeError> {
    if io::stdout().is_terminal() && io::stderr().is_terminal() {
        Ok(())
    } else {
        Err(RuntimeError::NonInteractiveTerminal)
    }
}

fn run_with_adapters<T, F>(
    terminal: &mut T,
    app: &mut TuiApp,
    fetcher: &mut F,
) -> Result<(), RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
{
    terminal.enter()?;

    let result = run_loop(terminal, app, fetcher);
    let cleanup_result = terminal.cleanup();

    match (result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => Err(RuntimeError::with_cleanup_failure(primary, cleanup)),
    }
}

impl RuntimeError {
    fn with_cleanup_failure(primary: RuntimeError, cleanup: RuntimeError) -> Self {
        let cleanup = match cleanup {
            RuntimeError::Terminal(error) => error,
            other => io::Error::other(other.to_string()),
        };

        RuntimeError::Cleanup {
            primary: Box::new(primary),
            cleanup,
        }
    }
}

fn run_loop<T, F>(terminal: &mut T, app: &mut TuiApp, fetcher: &mut F) -> Result<(), RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
{
    let mut fetch_scheduler = FetchScheduler::default();

    if app.configured_url.is_some() {
        fetch_scheduler.request_refresh(app, fetcher);
        fetch_scheduler.apply_ready_results(app, fetcher);
    }

    terminal.draw(app)?;
    let mut now = Instant::now();
    let mut next_refresh = now + app.refresh_interval;

    loop {
        if fetch_scheduler.apply_ready_results(app, fetcher) {
            terminal.draw(app)?;
        }

        let time_until_refresh = next_refresh.saturating_duration_since(now);
        let poll_timeout = time_until_refresh.min(FETCH_RESULT_POLL_INTERVAL);

        if terminal.poll_event(poll_timeout)? {
            now = Instant::now();
            match terminal.read_event()? {
                RuntimeEvent::Quit => break,
                RuntimeEvent::Refresh => {
                    if app.configured_url.is_some() {
                        fetch_scheduler.request_refresh(app, fetcher);
                        fetch_scheduler.apply_ready_results(app, fetcher);
                    }
                    next_refresh = now + app.refresh_interval;
                    terminal.draw(app)?;
                }
                RuntimeEvent::Ignored => {}
            }

            continue;
        }

        now += poll_timeout;
        if now >= next_refresh {
            if app.configured_url.is_some() {
                fetch_scheduler.request_refresh(app, fetcher);
                fetch_scheduler.apply_ready_results(app, fetcher);
            }
            next_refresh = now + app.refresh_interval;
            terminal.draw(app)?;
        }
    }

    Ok(())
}

#[derive(Debug, Default)]
struct FetchScheduler {
    in_flight: bool,
    follow_up_requested: bool,
}

impl FetchScheduler {
    fn request_refresh<F>(&mut self, app: &TuiApp, fetcher: &mut F)
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
    }

    fn apply_ready_results<F>(&mut self, app: &mut TuiApp, fetcher: &mut F) -> bool
    where
        F: MeasureFetchWorker,
    {
        let mut applied_result = false;

        while let Some(completion) = fetcher.try_recv_fetch() {
            self.in_flight = false;
            applied_result = true;

            match completion.result {
                Ok(payload) => {
                    let snapshot = sensors::parse_snapshot(&payload);
                    app.apply_success(snapshot, completion.duration, completion.completed_at);
                }
                Err(error) => {
                    app.apply_failure(error, completion.duration);
                }
            }

            if self.follow_up_requested {
                self.follow_up_requested = false;
                self.request_refresh(app, fetcher);
            }
        }

        applied_result
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
    fn try_recv_fetch(&mut self) -> Option<FetchCompletion>;
}

struct BackgroundMeasureFetcher {
    client: Client,
    sender: Sender<FetchCompletion>,
    receiver: Receiver<FetchCompletion>,
}

impl BackgroundMeasureFetcher {
    fn new(client: Client) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            client,
            sender,
            receiver,
        }
    }
}

impl MeasureFetchWorker for BackgroundMeasureFetcher {
    fn start_fetch(&mut self, base_url: Url) {
        let client = self.client.clone();
        let sender = self.sender.clone();

        tokio::spawn(async move {
            let started = Instant::now();
            let result = device::fetch_current_measures_with_client(&client, &base_url)
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(FetchCompletion {
                result,
                duration: started.elapsed(),
                completed_at: SystemTime::now(),
            });
        });
    }

    fn try_recv_fetch(&mut self) -> Option<FetchCompletion> {
        self.receiver.try_recv().ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeEvent {
    Quit,
    Refresh,
    Ignored,
}

trait TerminalRuntime {
    fn enter(&mut self) -> Result<(), RuntimeError>;
    fn draw(&mut self, app: &TuiApp) -> Result<(), RuntimeError>;
    fn poll_event(&mut self, timeout: Duration) -> Result<bool, RuntimeError>;
    fn read_event(&mut self) -> Result<RuntimeEvent, RuntimeError>;
    fn cleanup(&mut self) -> Result<(), RuntimeError>;
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

    fn poll_event(&mut self, timeout: Duration) -> Result<bool, RuntimeError> {
        Ok(event::poll(timeout)?)
    }

    fn read_event(&mut self) -> Result<RuntimeEvent, RuntimeError> {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => Ok(match key.code {
                KeyCode::Char('q') | KeyCode::Esc => RuntimeEvent::Quit,
                KeyCode::Char('r') | KeyCode::Char('R') => RuntimeEvent::Refresh,
                _ => RuntimeEvent::Ignored,
            }),
            _ => Ok(RuntimeEvent::Ignored),
        }
    }

    fn cleanup(&mut self) -> Result<(), RuntimeError> {
        match self.session.as_mut() {
            Some(session) => session.restore(),
            None => Ok(()),
        }
    }
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
    use serde_json::json;
    use std::collections::VecDeque;

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

        fn poll_event(&mut self, _timeout: Duration) -> Result<bool, RuntimeError> {
            self.calls.push(RuntimeCall::Poll);
            if let Some(result) = self.polls.pop_front() {
                return result.map_err(RuntimeError::from);
            }
            Ok(!self.events.is_empty())
        }

        fn read_event(&mut self) -> Result<RuntimeEvent, RuntimeError> {
            self.calls.push(RuntimeCall::Read);
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
    struct HarnessFetcher {
        completions: VecDeque<Option<Result<Value, String>>>,
        calls: Vec<Url>,
        active_fetch: bool,
    }

    impl HarnessFetcher {
        fn new(results: impl IntoIterator<Item = Result<Value, String>>) -> Self {
            Self {
                completions: results.into_iter().map(Some).collect(),
                calls: Vec::new(),
                active_fetch: false,
            }
        }

        fn pending_then(results: impl IntoIterator<Item = Option<Result<Value, String>>>) -> Self {
            Self {
                completions: results.into_iter().collect(),
                calls: Vec::new(),
                active_fetch: false,
            }
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

        fn try_recv_fetch(&mut self) -> Option<FetchCompletion> {
            if !self.active_fetch {
                return None;
            }

            let result = self.completions.pop_front()??;
            self.active_fetch = false;
            Some(FetchCompletion {
                result,
                duration: Duration::from_millis(25),
                completed_at: SystemTime::UNIX_EPOCH + Duration::from_secs(60),
            })
        }
    }

    fn app(configured_url: Option<Url>) -> TuiApp {
        TuiApp::with_client(
            configured_url,
            Duration::from_secs(30),
            FetchSettings::default(),
            Client::new(),
        )
    }

    fn configured_url() -> Url {
        Url::parse("http://192.168.1.201/").expect("test URL should parse")
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
            RuntimeError::Cleanup { primary, .. } => terminal_error_message(primary),
        }
    }

    fn cleanup_error_message(error: &RuntimeError) -> Option<String> {
        match error {
            RuntimeError::Cleanup { cleanup, .. } => Some(cleanup.to_string()),
            _ => None,
        }
    }

    #[tokio::test]
    async fn harness_drives_normal_quit_without_fetch() {
        let mut terminal = HarnessTerminal::with_quit();
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
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
        let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]);
        for _ in 0..50 {
            terminal.polls.push_back(Ok(false));
        }
        let mut fetcher =
            HarnessFetcher::new([Ok(successful_payload()), Ok(later_successful_payload())]);
        let url = configured_url();
        let mut app = app(Some(url.clone()));
        app.refresh_interval = Duration::from_secs(5);

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
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
    async fn failure_after_success_preserves_last_successful_snapshot() {
        let mut terminal =
            HarnessTerminal::with_events([RuntimeEvent::Refresh, RuntimeEvent::Quit]);
        let mut fetcher =
            HarnessFetcher::new([Ok(successful_payload()), Err("refresh failed".to_owned())]);
        let url = configured_url();
        let mut app = app(Some(url.clone()));

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
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
    async fn quit_while_fetch_is_pending_does_not_wait_for_completion() {
        let mut terminal = HarnessTerminal::with_quit();
        let mut fetcher = HarnessFetcher::pending_then([None]);
        let mut app = app(Some(configured_url()));

        run_with_adapters(&mut terminal, &mut app, &mut fetcher)
            .expect("runtime should quit cleanly");

        assert_eq!(fetcher.calls.len(), 1);
        assert_eq!(terminal.calls.last(), Some(&RuntimeCall::Cleanup));
        assert_eq!(app.current_snapshot, None);
    }

    #[tokio::test]
    async fn cleanup_runs_after_draw_failure() {
        let mut terminal = HarnessTerminal::with_quit().fail_draw("draw failed");
        let mut fetcher = HarnessFetcher::new([]);
        let mut app = app(None);

        let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
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
