//! The crossterm/ratatui terminal adapter, plus the key-to-event mapping.
//!
//! [`TerminalRuntime`] is the seam the event loop is written against;
//! [`CrosstermRuntime`] is the only production implementation, and
//! [`TerminalSession`] owns the raw-mode/alternate-screen state that must be
//! undone in the right order no matter how the process leaves the loop.

use std::{
    io::{self, Stdout},
    time::Duration,
};

use crossterm::{
    cursor::Show,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::tui::app::TuiApp;

use super::{
    RuntimeError,
    event::{InputMode, RuntimeEvent},
    keymap,
};
use crate::tui::ui::{HitMap, HitTarget};

pub(super) trait TerminalRuntime {
    fn enter(&mut self) -> Result<(), RuntimeError>;
    fn draw(&mut self, app: &TuiApp) -> Result<(), RuntimeError>;
    async fn poll_event(&mut self, timeout: Duration) -> Result<bool, RuntimeError>;
    async fn read_event(&mut self, mode: InputMode) -> Result<RuntimeEvent, RuntimeError>;
    fn cleanup(&mut self) -> Result<(), RuntimeError>;

    /// What was drawn at a terminal cell, for resolving a mouse click.
    ///
    /// Defaults to "nothing": an implementation that never renders — every
    /// test fake — has no geometry to test against, and a click that resolves
    /// to nothing is correctly a no-op.
    fn hit_test(&self, _column: u16, _row: u16) -> Option<HitTarget> {
        None
    }
}

#[derive(Default)]
pub(super) struct CrosstermRuntime {
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
        Ok(match blocking_terminal_call(event::read).await? {
            // Key *release* events also arrive on some terminals; acting on
            // both would fire every shortcut twice.
            Event::Key(key) if key.kind == KeyEventKind::Press => keymap::key_event(key, mode),
            Event::Mouse(mouse) => keymap::mouse_event(mouse, mode),
            _ => RuntimeEvent::Ignored,
        })
    }

    fn cleanup(&mut self) -> Result<(), RuntimeError> {
        match self.session.as_mut() {
            Some(session) => session.restore(),
            None => Ok(()),
        }
    }

    fn hit_test(&self, column: u16, row: u16) -> Option<HitTarget> {
        self.session
            .as_ref()
            .and_then(|session| session.hits.hit(column, row))
    }
}

/// Runs a blocking crossterm call off the async executor. `event::poll` and
/// `event::read` park the thread, which would stall the fetch tasks sharing a
/// current-thread runtime.
pub(super) async fn blocking_terminal_call<T>(
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

/// The terminal state this process turned on, and therefore owes the user
/// back. Each flag is cleared only once its undo actually succeeded, so a
/// partial teardown does not claim to have restored more than it did.
/// Which of the terminal modifications are currently in effect.
///
/// One named struct rather than three `bool` parameters in a row: the teardown
/// steps depend on all three, and transposing two of them at a call site
/// compiles cleanly while producing a restore that leaves the shell in raw
/// mode, or writes mouse-capture escapes onto the user's shell after the
/// alternate screen has already been left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct TerminalState {
    pub(super) raw_mode: bool,
    pub(super) alternate_screen: bool,
    pub(super) mouse_capture: bool,
}

impl TerminalState {
    /// Nothing entered yet — what a session starts from and ends at.
    pub(super) const NONE: Self = Self {
        raw_mode: false,
        alternate_screen: false,
        mouse_capture: false,
    };
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    state: TerminalState,
    /// Click targets from the most recent frame, used to resolve mouse
    /// coordinates back to the row the user clicked.
    hits: HitMap,
}

impl TerminalSession {
    /// Enters raw mode and the alternate screen, unwinding whatever already
    /// succeeded if a later step fails — otherwise a failed launch would exit
    /// leaving the user's shell in raw mode.
    fn enter() -> Result<Self, RuntimeError> {
        enable_raw_mode()?;
        let mut state = TerminalState {
            raw_mode: true,
            ..TerminalState::NONE
        };

        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            cleanup_failed_terminal_setup(&mut stdout, state);
            return Err(error.into());
        }
        state.alternate_screen = true;

        // Mouse capture is best-effort: a terminal that refuses it still gets
        // a fully keyboard-driven TUI, which is worse than nothing only if we
        // aborted the launch over it.
        state.mouse_capture = execute!(stdout, EnableMouseCapture).is_ok();

        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut stdout = io::stdout();
                cleanup_failed_terminal_setup(&mut stdout, state);
                return Err(error.into());
            }
        };

        Ok(Self {
            terminal,
            state,
            hits: HitMap::default(),
        })
    }

    fn draw(&mut self, app: &TuiApp) -> Result<(), RuntimeError> {
        let hits = &mut self.hits;
        self.terminal
            .draw(|frame| crate::tui::ui::draw_with_hits(frame, app, hits))?;
        Ok(())
    }

    /// Runs every applicable cleanup step even if an earlier one failed, and
    /// reports the first error. Stopping at the first failure would strand the
    /// terminal in a worse state than finishing the teardown.
    fn restore(&mut self) -> Result<(), RuntimeError> {
        let mut first_error = None;
        let cleanup_steps = terminal_cleanup_steps(self.state);

        for step in cleanup_steps {
            match step {
                TerminalCleanupStep::DisableMouseCapture => {
                    if let Err(error) = execute!(self.terminal.backend_mut(), DisableMouseCapture) {
                        first_error.get_or_insert(error);
                    } else {
                        self.state.mouse_capture = false;
                    }
                }
                TerminalCleanupStep::LeaveAlternateScreen => {
                    if let Err(error) = execute!(self.terminal.backend_mut(), LeaveAlternateScreen)
                    {
                        first_error.get_or_insert(error);
                    } else {
                        self.state.alternate_screen = false;
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
                        self.state.raw_mode = false;
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
pub(super) enum TerminalCleanupStep {
    DisableMouseCapture,
    LeaveAlternateScreen,
    ShowCursor,
    DisableRawMode,
}

/// The teardown steps for a given amount of setup, in the order they must
/// run: leave the alternate screen and show the cursor *before* leaving raw
/// mode, so the cursor is restored on the screen the user is returning to.
pub(super) fn terminal_cleanup_steps(state: TerminalState) -> Vec<TerminalCleanupStep> {
    let mut steps = Vec::new();

    // Mouse capture goes first: it is written to the alternate screen, so
    // releasing it after leaving would emit the escape sequence onto the
    // user's restored shell instead.
    if state.mouse_capture {
        steps.push(TerminalCleanupStep::DisableMouseCapture);
    }

    if state.alternate_screen {
        steps.push(TerminalCleanupStep::LeaveAlternateScreen);
    }

    if state.raw_mode || state.alternate_screen {
        steps.push(TerminalCleanupStep::ShowCursor);
    }

    if state.raw_mode {
        steps.push(TerminalCleanupStep::DisableRawMode);
    }

    steps
}

/// Best-effort teardown for a setup that never completed. Errors are dropped
/// deliberately: the caller is already returning the failure that caused this,
/// and a cleanup error on top of it would bury the useful one.
fn cleanup_failed_terminal_setup<W: io::Write>(writer: &mut W, state: TerminalState) {
    for step in terminal_cleanup_steps(state) {
        match step {
            TerminalCleanupStep::DisableMouseCapture => {
                let _ = execute!(writer, DisableMouseCapture);
            }
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

/// Last line of defense: a panic unwinding past the runtime still restores
/// the terminal, because `restore` is idempotent through its own flags.
impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
