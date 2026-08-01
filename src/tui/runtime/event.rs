//! The loop's input vocabulary: what a keypress means, and in which mode.
//!
//! Kept apart from the loop that consumes these so the key bindings in
//! [`super::terminal`] and the handlers in [`super::event_loop`] can be read
//! against one shared definition.

use crate::tui::app::{TuiApp, View};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeEvent {
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
pub(super) enum InputMode {
    /// Dashboard: single-key shortcuts (`q`, `r`, `+`/`-`, `:`, `t`, `c`).
    Normal,
    /// A modal view (theme picker / config editor) is open but no text
    /// field is being edited: arrows navigate, Enter acts, Esc closes.
    ModalNav,
    /// A text field (palette input, or a config-editor field mid-edit) is
    /// capturing every printable character.
    TextEntry,
}

pub(super) fn input_mode(app: &TuiApp) -> InputMode {
    match app.view {
        View::Dashboard => InputMode::Normal,
        View::CommandPalette => InputMode::TextEntry,
        View::ConfigEditor if app.config_editor_editing.is_some() => InputMode::TextEntry,
        View::ConfigEditor | View::ThemeSettings => InputMode::ModalNav,
    }
}
