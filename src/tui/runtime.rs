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
use tokio::task::JoinHandle;
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
    let mut app = TuiApp::new(
        effective_config.configured_url,
        effective_config.refresh_interval,
    );

    if let Some(error) = effective_config.current_error {
        app.current_error = Some(error);
    }

    let mut terminal = CrosstermRuntime::default();
    let mut fetcher = BackgroundMeasureFetcher::new(fetch_client);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher).await
}

fn ensure_interactive_terminal() -> Result<(), RuntimeError> {
    if io::stdout().is_terminal() && io::stderr().is_terminal() {
        Ok(())
    } else {
        Err(RuntimeError::NonInteractiveTerminal)
    }
}

async fn run_with_adapters<T, F>(
    terminal: &mut T,
    app: &mut TuiApp,
    fetcher: &mut F,
) -> Result<(), RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
{
    terminal.enter()?;

    let mut clock = SystemClock;
    let result = run_loop(terminal, app, fetcher, &mut clock).await;
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

async fn run_loop<T, F, C>(
    terminal: &mut T,
    app: &mut TuiApp,
    fetcher: &mut F,
    clock: &mut C,
) -> Result<(), RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
    C: RuntimeClock,
{
    let mut fetch_scheduler = FetchScheduler::default();

    let result = async {
        if app.configured_url.is_some() {
            fetch_scheduler.request_refresh(app, fetcher);
            fetch_scheduler.apply_ready_results(app, fetcher);
        }

        terminal.draw(app)?;
        let mut now = clock.now();
        let mut next_refresh = now + app.refresh_interval;

        loop {
            if fetch_scheduler.apply_ready_results(app, fetcher) {
                terminal.draw(app)?;
            }

            now = clock.now();
            let time_until_refresh = next_refresh.saturating_duration_since(now);
            let poll_timeout = time_until_refresh.min(FETCH_RESULT_POLL_INTERVAL);

            if terminal.poll_event(poll_timeout).await? {
                let _poll_returned_at = clock.now();
                match terminal.read_event().await? {
                    RuntimeEvent::Quit => break,
                    RuntimeEvent::Refresh => {
                        now = clock.now();
                        if app.configured_url.is_some() {
                            fetch_scheduler.request_refresh(app, fetcher);
                            fetch_scheduler.apply_ready_results(app, fetcher);
                        }
                        next_refresh = now + app.refresh_interval;
                        terminal.draw(app)?;
                    }
                    RuntimeEvent::Ignored => {}
                }

                now = clock.now();
                if now >= next_refresh {
                    if app.configured_url.is_some() {
                        fetch_scheduler.request_refresh(app, fetcher);
                        fetch_scheduler.apply_ready_results(app, fetcher);
                    }
                    next_refresh = now + app.refresh_interval;
                    terminal.draw(app)?;
                }

                continue;
            }

            now = clock.now();
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
    .await;

    fetch_scheduler.cancel_pending_fetch(app, fetcher);

    result
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

    fn apply_ready_results<F>(&mut self, app: &mut TuiApp, fetcher: &mut F) -> bool
    where
        F: MeasureFetchWorker,
    {
        if !self.in_flight {
            while fetcher.try_recv_fetch().is_some() {}
            return false;
        }

        let mut applied_result = false;

        while let Some(completion) = fetcher.try_recv_fetch() {
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

        applied_result
    }

    fn cancel_pending_fetch<F>(&mut self, app: &mut TuiApp, fetcher: &mut F)
    where
        F: MeasureFetchWorker,
    {
        if self.in_flight {
            fetcher.cancel_fetch();
        }

        self.in_flight = false;
        self.follow_up_requested = false;
        app.is_fetching = false;
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
    fn cancel_fetch(&mut self);
}

struct BackgroundMeasureFetcher {
    client: Client,
    sender: Sender<BackgroundFetchMessage>,
    receiver: Receiver<BackgroundFetchMessage>,
    active_request_id: Option<u64>,
    next_request_id: u64,
    active_handle: Option<JoinHandle<()>>,
}

impl BackgroundMeasureFetcher {
    fn new(client: Client) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            client,
            sender,
            receiver,
            active_request_id: None,
            next_request_id: 0,
            active_handle: None,
        }
    }
}

#[derive(Debug)]
struct BackgroundFetchMessage {
    request_id: u64,
    completion: FetchCompletion,
}

impl MeasureFetchWorker for BackgroundMeasureFetcher {
    fn start_fetch(&mut self, base_url: Url) {
        self.cancel_fetch();

        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        self.active_request_id = Some(request_id);

        let client = self.client.clone();
        let sender = self.sender.clone();

        self.active_handle = Some(tokio::spawn(async move {
            let started = Instant::now();
            let result = device::fetch_current_measures_with_client(&client, &base_url)
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(BackgroundFetchMessage {
                request_id,
                completion: FetchCompletion {
                    result,
                    duration: started.elapsed(),
                    completed_at: SystemTime::now(),
                },
            });
        }));
    }

    fn try_recv_fetch(&mut self) -> Option<FetchCompletion> {
        while let Ok(message) = self.receiver.try_recv() {
            if Some(message.request_id) == self.active_request_id {
                self.active_request_id = None;
                self.active_handle = None;
                return Some(message.completion);
            }
        }

        None
    }

    fn cancel_fetch(&mut self) {
        self.active_request_id = None;

        if let Some(handle) = self.active_handle.take() {
            handle.abort();
        }

        while self.receiver.try_recv().is_ok() {}
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
    async fn poll_event(&mut self, timeout: Duration) -> Result<bool, RuntimeError>;
    async fn read_event(&mut self) -> Result<RuntimeEvent, RuntimeError>;
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

    async fn poll_event(&mut self, timeout: Duration) -> Result<bool, RuntimeError> {
        blocking_terminal_call(move || event::poll(timeout)).await
    }

    async fn read_event(&mut self) -> Result<RuntimeEvent, RuntimeError> {
        let event = blocking_terminal_call(event::read).await?;
        match event {
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
    use std::{
        cell::Cell,
        collections::VecDeque,
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
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

        async fn read_event(&mut self) -> Result<RuntimeEvent, RuntimeError> {
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
        let result = run_loop(terminal, app, fetcher, &mut clock).await;
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
    }

    impl HarnessFetcher {
        fn new(results: impl IntoIterator<Item = Result<Value, String>>) -> Self {
            Self {
                completions: results.into_iter().map(Some).collect(),
                stale_completions_after_cancel: VecDeque::new(),
                calls: Vec::new(),
                active_fetch: false,
                canceled_fetches: 0,
            }
        }

        fn pending_then(results: impl IntoIterator<Item = Option<Result<Value, String>>>) -> Self {
            Self {
                completions: results.into_iter().collect(),
                stale_completions_after_cancel: VecDeque::new(),
                calls: Vec::new(),
                active_fetch: false,
                canceled_fetches: 0,
            }
        }

        fn with_stale_after_cancel(
            mut self,
            results: impl IntoIterator<Item = Result<Value, String>>,
        ) -> Self {
            self.stale_completions_after_cancel = results.into_iter().collect();
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

        fn try_recv_fetch(&mut self) -> Option<FetchCompletion> {
            if !self.active_fetch {
                return self
                    .stale_completions_after_cancel
                    .pop_front()
                    .map(|result| FetchCompletion {
                        result,
                        duration: Duration::from_millis(25),
                        completed_at: SystemTime::UNIX_EPOCH + Duration::from_secs(60),
                    });
            }

            let result = self.completions.pop_front()??;
            self.active_fetch = false;
            Some(FetchCompletion {
                result,
                duration: Duration::from_millis(25),
                completed_at: SystemTime::UNIX_EPOCH + Duration::from_secs(60),
            })
        }

        fn cancel_fetch(&mut self) {
            if self.active_fetch {
                self.canceled_fetches += 1;
                self.active_fetch = false;
            }
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

        async fn read_event(&mut self) -> Result<RuntimeEvent, RuntimeError> {
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

        fn try_recv_fetch(&mut self) -> Option<FetchCompletion> {
            self.receiver.try_recv().ok()
        }

        fn cancel_fetch(&mut self) {
            if let Some(handle) = self.active_handle.take() {
                handle.abort();
            }
            while self.receiver.try_recv().is_ok() {}
        }
    }

    fn app(configured_url: Option<Url>) -> TuiApp {
        TuiApp::new(configured_url, Duration::from_secs(30))
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
    async fn quit_while_fetch_is_pending_does_not_wait_for_completion() {
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

    #[test]
    fn stale_completion_after_cancellation_does_not_mutate_app() {
        let url = configured_url();
        let mut app = app(Some(url.clone()));
        let mut fetcher = HarnessFetcher::pending_then([None])
            .with_stale_after_cancel([Ok(successful_payload())]);
        let mut scheduler = FetchScheduler::default();

        scheduler.request_refresh(&mut app, &mut fetcher);
        scheduler.cancel_pending_fetch(&mut app, &mut fetcher);

        assert!(!scheduler.apply_ready_results(&mut app, &mut fetcher));
        assert_eq!(fetcher.calls, [url]);
        assert_eq!(fetcher.canceled_fetches, 1);
        assert_eq!(app.current_snapshot, None);
        assert_eq!(app.current_error, None);
        assert!(!app.is_fetching);
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
