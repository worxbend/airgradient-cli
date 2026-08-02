//! Terminal doubles for the runtime event loop.
//!
//! [`HarnessTerminal`] replays a scripted list of events and can be told to
//! fail any single call, which is how the failure-path tests reach the
//! cleanup and cancellation branches without a real terminal.
//! [`BlockingPollTerminal`] instead parks a real thread inside `poll_event`,
//! proving the loop keeps driving fetches while input blocks.

use std::{
    cell::Cell,
    collections::VecDeque,
    io,
    rc::Rc,
    time::{Duration, Instant},
};

use crate::tui::{
    app::TuiApp,
    runtime::{
        RuntimeError,
        event::{InputMode, RuntimeEvent},
        terminal::{TerminalRuntime, blocking_terminal_call},
    },
    ui::HitTarget,
};

use super::test_clock_start;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeCall {
    Enter,
    Draw,
    Poll,
    Read,
    Cleanup,
}

#[derive(Debug)]
pub(super) struct HarnessTerminal {
    pub(super) events: VecDeque<RuntimeEvent>,
    pub(super) polls: VecDeque<Result<bool, io::Error>>,
    pub(super) clock: Rc<Cell<Instant>>,
    pub(super) poll_timeouts: Vec<Duration>,
    pub(super) poll_advances: VecDeque<Duration>,
    pub(super) read_advances: VecDeque<Duration>,
    pub(super) draw_error: Option<io::Error>,
    pub(super) read_error: Option<io::Error>,
    pub(super) cleanup_error: Option<io::Error>,
    pub(super) calls: Vec<RuntimeCall>,
    pub(super) drawn_errors: Vec<Option<String>>,
    pub(super) cleanup_called: bool,
    /// What `hit_test` should report, standing in for the geometry a real
    /// render would have recorded.
    pub(super) hit: Option<HitTarget>,
}

impl HarnessTerminal {
    pub(super) fn with_events(events: impl IntoIterator<Item = RuntimeEvent>) -> Self {
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
            hit: None,
        }
    }

    pub(super) fn with_hit(mut self, target: HitTarget) -> Self {
        self.hit = Some(target);
        self
    }

    pub(super) fn with_quit() -> Self {
        Self::with_events([RuntimeEvent::Quit])
    }

    pub(super) fn fail_draw(mut self, message: &'static str) -> Self {
        self.draw_error = Some(io::Error::other(message));
        self
    }

    pub(super) fn fail_poll(mut self, message: &'static str) -> Self {
        self.polls.push_back(Err(io::Error::other(message)));
        self
    }

    pub(super) fn fail_read(mut self, message: &'static str) -> Self {
        self.read_error = Some(io::Error::other(message));
        self
    }

    pub(super) fn fail_cleanup(mut self, message: &'static str) -> Self {
        self.cleanup_error = Some(io::Error::other(message));
        self
    }

    pub(super) fn with_clock(mut self, clock: Rc<Cell<Instant>>) -> Self {
        self.clock = clock;
        self
    }

    pub(super) fn poll_advance(mut self, duration: Duration) -> Self {
        self.poll_advances.push_back(duration);
        self
    }

    pub(super) fn read_advance(mut self, duration: Duration) -> Self {
        self.read_advances.push_back(duration);
        self
    }

    pub(super) fn advance_clock(&self, duration: Duration) {
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

    fn hit_test(&self, _column: u16, _row: u16) -> Option<HitTarget> {
        self.hit
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
pub(super) struct BlockingPollTerminal {
    pub(super) poll_delay: Duration,
    pub(super) cleanup_called: bool,
}

impl BlockingPollTerminal {
    pub(super) fn new(poll_delay: Duration) -> Self {
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
