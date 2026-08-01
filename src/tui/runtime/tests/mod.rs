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
mod fetch_fakes;
mod loop_flow;
mod schedule;
mod terminal_fakes;

use fetch_fakes::*;
use terminal_fakes::*;

use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use url::Url;

use crate::{
    config::{MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS},
    tui::{app::TuiApp, theme},
};

use super::{
    RuntimeError,
    effective_config::runtime_refresh_schedule_interval,
    event::RuntimeEvent,
    event_loop::{FETCH_RESULT_POLL_INTERVAL, run_loop, run_splash},
    fetch::{FetchScheduler, MeasureFetchWorker, observe_fetch_handle},
    run_with_adapters, run_with_adapters_with_refresh_interval,
    schedule::RuntimeClock,
    terminal::{TerminalCleanupStep, TerminalRuntime, terminal_cleanup_steps},
};

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
