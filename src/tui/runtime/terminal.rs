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
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::tui::app::TuiApp;

use super::{
    RuntimeError,
    event::{InputMode, RuntimeEvent},
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
            Event::Key(key) if key.kind == KeyEventKind::Press => key_event(key, mode),
            Event::Mouse(mouse) => mouse_event(mouse, mode),
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

/// Maps a keypress to a runtime event for the currently active input mode.
///
/// The same key means different things per mode by design — in
/// [`InputMode::TextEntry`] every printable character is content, so the
/// dashboard shortcuts (`q`, `t`, `c`, …) must not be reachable there.
fn key_event(key: KeyEvent, mode: InputMode) -> RuntimeEvent {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match mode {
        // Vim's insert-mode line editing: everything printable is content,
        // and only the control chords act.
        InputMode::TextEntry => match key.code {
            KeyCode::Esc => RuntimeEvent::Escape,
            KeyCode::Enter => RuntimeEvent::Confirm,
            KeyCode::Backspace => RuntimeEvent::PaletteBackspace,
            KeyCode::Char('w') if ctrl => RuntimeEvent::DeleteWordBefore,
            KeyCode::Char('u') if ctrl => RuntimeEvent::ClearLine,
            KeyCode::Char('c') if ctrl => RuntimeEvent::Escape,
            KeyCode::Char(c) => RuntimeEvent::PaletteChar(c),
            _ => RuntimeEvent::Ignored,
        },
        // The which-key popup owns the next key, whatever it is.
        InputMode::Leader => match key.code {
            KeyCode::Esc => RuntimeEvent::Escape,
            KeyCode::Char(' ') => RuntimeEvent::ToggleLeader,
            KeyCode::Char(c) => RuntimeEvent::LeaderKey(c),
            _ => RuntimeEvent::Escape,
        },
        InputMode::ModalNav => match key.code {
            KeyCode::Char('d') if ctrl => RuntimeEvent::NavHalfPage(1),
            KeyCode::Char('u') if ctrl => RuntimeEvent::NavHalfPage(-1),
            KeyCode::Char('c') if ctrl => RuntimeEvent::Escape,
            KeyCode::Esc | KeyCode::F(2) | KeyCode::Char('q' | 'c' | 'C') => RuntimeEvent::Escape,
            KeyCode::Up | KeyCode::Char('k') => RuntimeEvent::NavUp,
            KeyCode::Down | KeyCode::Char('j') => RuntimeEvent::NavDown,
            // `g` is a prefix; the event loop pairs it with the next one.
            KeyCode::Char('g') => RuntimeEvent::GPrefix,
            KeyCode::Char('G') => RuntimeEvent::NavLast,
            KeyCode::Home => RuntimeEvent::NavFirst,
            KeyCode::End => RuntimeEvent::NavLast,
            KeyCode::PageDown => RuntimeEvent::NavHalfPage(1),
            KeyCode::PageUp => RuntimeEvent::NavHalfPage(-1),
            // `l`/`Right` enters, matching vim tree/file pickers.
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => RuntimeEvent::Confirm,
            _ => RuntimeEvent::Ignored,
        },
        InputMode::Normal => match key.code {
            KeyCode::Char('c') if ctrl => RuntimeEvent::Quit,
            KeyCode::Char(' ') => RuntimeEvent::ToggleLeader,
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

/// Maps a mouse action to a runtime event.
///
/// The wheel scrolls the active list, and a left click selects what it landed
/// on. Text entry ignores the mouse entirely: there is no cursor to move
/// within the line, so a click there would only be able to do something
/// surprising.
fn mouse_event(mouse: MouseEvent, mode: InputMode) -> RuntimeEvent {
    match mode {
        InputMode::TextEntry | InputMode::Leader => RuntimeEvent::Ignored,
        InputMode::Normal | InputMode::ModalNav => match mouse.kind {
            MouseEventKind::ScrollUp => RuntimeEvent::NavUp,
            MouseEventKind::ScrollDown => RuntimeEvent::NavDown,
            MouseEventKind::Down(MouseButton::Left) => {
                RuntimeEvent::MouseClick(mouse.column, mouse.row)
            }
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
    mouse_capture_enabled: bool,
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
        let raw_mode_enabled = true;

        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            cleanup_failed_terminal_setup(&mut stdout, raw_mode_enabled, false, false);
            return Err(error.into());
        }

        // Mouse capture is best-effort: a terminal that refuses it still gets
        // a fully keyboard-driven TUI, which is worse than nothing only if we
        // aborted the launch over it.
        let mouse_capture_enabled = execute!(stdout, EnableMouseCapture).is_ok();

        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut stdout = io::stdout();
                cleanup_failed_terminal_setup(
                    &mut stdout,
                    raw_mode_enabled,
                    true,
                    mouse_capture_enabled,
                );
                return Err(error.into());
            }
        };

        Ok(Self {
            terminal,
            raw_mode_enabled,
            alternate_screen_enabled: true,
            mouse_capture_enabled,
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
        let cleanup_steps = terminal_cleanup_steps(
            self.raw_mode_enabled,
            self.alternate_screen_enabled,
            self.mouse_capture_enabled,
        );

        for step in cleanup_steps {
            match step {
                TerminalCleanupStep::DisableMouseCapture => {
                    if let Err(error) = execute!(self.terminal.backend_mut(), DisableMouseCapture) {
                        first_error.get_or_insert(error);
                    } else {
                        self.mouse_capture_enabled = false;
                    }
                }
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
    DisableMouseCapture,
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
    mouse_capture_enabled: bool,
) -> Vec<TerminalCleanupStep> {
    let mut steps = Vec::new();

    // Mouse capture goes first: it is written to the alternate screen, so
    // releasing it after leaving would emit the escape sequence onto the
    // user's restored shell instead.
    if mouse_capture_enabled {
        steps.push(TerminalCleanupStep::DisableMouseCapture);
    }

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
    mouse_capture_enabled: bool,
) {
    for step in terminal_cleanup_steps(
        raw_mode_enabled,
        alternate_screen_enabled,
        mouse_capture_enabled,
    ) {
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
