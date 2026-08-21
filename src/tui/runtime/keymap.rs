//! Which `RuntimeEvent` a keypress or click means, per [`InputMode`].
//!
//! A lookup table and nothing else: it owns no terminal state, opens nothing
//! and restores nothing. Changing a key binding is a frequent edit, and this
//! keeps it out of [`super::terminal`], whose raw-mode and alternate-screen
//! teardown runs in a carefully chosen order that nobody should have to scroll
//! past to rename a shortcut.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::event::{InputMode, RuntimeEvent};

/// Maps a keypress to a runtime event for the currently active input mode.
///
/// The same key means different things per mode by design — in
/// [`InputMode::TextEntry`] every printable character is content, so the
/// dashboard shortcuts (`q`, `t`, `c`, …) must not be reachable there.
pub(super) fn key_event(key: KeyEvent, mode: InputMode) -> RuntimeEvent {
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
            // `h` closes as well as `q`/`c`/Esc — it is vim's "back out"
            // direction, and it is what opened the chart in the first place.
            KeyCode::Esc | KeyCode::F(2) | KeyCode::Char('q' | 'c' | 'C' | 'h' | 'H') => {
                RuntimeEvent::Escape
            }
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
            KeyCode::Char('h' | 'H') => RuntimeEvent::ToggleHistory,
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
pub(super) fn mouse_event(mouse: MouseEvent, mode: InputMode) -> RuntimeEvent {
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
