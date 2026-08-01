//! Shared fakes for the runtime's two seams.
//!
//! The event loop is written against [`TerminalRuntime`] and
//! [`MeasureFetchWorker`], so every test here drives it with scripted
//! in-memory doubles: no real terminal, no network, and a clock the test
//! advances itself. That is what keeps these tests fast and deterministic
//! instead of sleeping through real refresh intervals.
//!
//! The test cases themselves live in the child modules, grouped by what they
//! exercise: [`schedule`] (interval policy), [`loop_flow`] (the happy path
//! and refresh timing), and [`failure_paths`] (I/O failures, cancellation,
//! and terminal cleanup).

mod failure_paths;
mod loop_flow;
mod schedule;

use std::{
    cell::Cell,
    collections::VecDeque,
    io,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant, SystemTime},
};

use serde_json::{Value, json};
use tokio::task::JoinHandle;
use url::Url;

use crate::{
    config::{MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS},
    tui::{app::TuiApp, theme},
};

use super::{
    RuntimeError,
    effective_config::runtime_refresh_schedule_interval,
    event_loop::{
        FETCH_RESULT_POLL_INTERVAL, InputMode, RuntimeClock, RuntimeEvent, run_loop, run_splash,
    },
    fetch::{FetchCompletion, FetchScheduler, MeasureFetchWorker, observe_fetch_handle},
    run_with_adapters, run_with_adapters_with_refresh_interval,
    terminal::{
        TerminalCleanupStep, TerminalRuntime, blocking_terminal_call, terminal_cleanup_steps,
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
        (Err(primary), Err(cleanup)) => Err(RuntimeError::with_cleanup_failure(primary, cleanup)),
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
