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
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::tui::app::TuiApp;

use super::{RuntimeError, event_loop::InputMode, event_loop::RuntimeEvent};

pub(super) trait TerminalRuntime {
    fn enter(&mut self) -> Result<(), RuntimeError>;
    fn draw(&mut self, app: &TuiApp) -> Result<(), RuntimeError>;
    async fn poll_event(&mut self, timeout: Duration) -> Result<bool, RuntimeError>;
    async fn read_event(&mut self, mode: InputMode) -> Result<RuntimeEvent, RuntimeError>;
    fn cleanup(&mut self) -> Result<(), RuntimeError>;
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
        let event = blocking_terminal_call(event::read).await?;
        let Event::Key(key) = event else {
            return Ok(RuntimeEvent::Ignored);
        };
        // Key *release* events also arrive on some terminals; acting on both
        // would fire every shortcut twice.
        if key.kind != KeyEventKind::Press {
            return Ok(RuntimeEvent::Ignored);
        }

        Ok(key_event(key.code, mode))
    }

    fn cleanup(&mut self) -> Result<(), RuntimeError> {
        match self.session.as_mut() {
            Some(session) => session.restore(),
            None => Ok(()),
        }
    }
}

/// Maps a keypress to a runtime event for the currently active input mode.
///
/// The same key means different things per mode by design — in
/// [`InputMode::TextEntry`] every printable character is content, so the
/// dashboard shortcuts (`q`, `t`, `c`, …) must not be reachable there.
fn key_event(code: KeyCode, mode: InputMode) -> RuntimeEvent {
    match mode {
        InputMode::TextEntry => match code {
            KeyCode::Esc => RuntimeEvent::Escape,
            KeyCode::Enter => RuntimeEvent::Confirm,
            KeyCode::Backspace => RuntimeEvent::PaletteBackspace,
            KeyCode::Char(c) => RuntimeEvent::PaletteChar(c),
            _ => RuntimeEvent::Ignored,
        },
        InputMode::ModalNav => match code {
            KeyCode::Esc | KeyCode::F(2) | KeyCode::Char('q' | 'c' | 'C') => RuntimeEvent::Escape,
            KeyCode::Up | KeyCode::Char('k') => RuntimeEvent::NavUp,
            KeyCode::Down | KeyCode::Char('j') => RuntimeEvent::NavDown,
            KeyCode::Enter => RuntimeEvent::Confirm,
            _ => RuntimeEvent::Ignored,
        },
        InputMode::Normal => match code {
            KeyCode::Char('q') | KeyCode::Esc => RuntimeEvent::Quit,
            KeyCode::Char('r' | 'R') => RuntimeEvent::Refresh,
            KeyCode::Char('+' | '=') => RuntimeEvent::IncreaseRefreshInterval,
            KeyCode::Char('-' | '_') => RuntimeEvent::DecreaseRefreshInterval,
            KeyCode::Char(':') => RuntimeEvent::OpenPalette,
            KeyCode::Char('t' | 'T') | KeyCode::F(2) => RuntimeEvent::ToggleThemeSettings,
            KeyCode::Char('c' | 'C') => RuntimeEvent::ToggleConfigEditor,
            _ => RuntimeEvent::Ignored,
        },
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
struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    raw_mode_enabled: bool,
    alternate_screen_enabled: bool,
}

impl TerminalSession {
    /// Enters raw mode and the alternate screen, unwinding whatever already
    /// succeeded if a later step fails — otherwise a failed launch would exit
    /// leaving the user's shell in raw mode.
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

    /// Runs every applicable cleanup step even if an earlier one failed, and
    /// reports the first error. Stopping at the first failure would strand the
    /// terminal in a worse state than finishing the teardown.
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
pub(super) enum TerminalCleanupStep {
    LeaveAlternateScreen,
    ShowCursor,
    DisableRawMode,
}

/// The teardown steps for a given amount of setup, in the order they must
/// run: leave the alternate screen and show the cursor *before* leaving raw
/// mode, so the cursor is restored on the screen the user is returning to.
pub(super) fn terminal_cleanup_steps(
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

/// Best-effort teardown for a setup that never completed. Errors are dropped
/// deliberately: the caller is already returning the failure that caused this,
/// and a cleanup error on top of it would bury the useful one.
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

/// Last line of defense: a panic unwinding past the runtime still restores
/// the terminal, because `restore` is idempotent through its own flags.
impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
