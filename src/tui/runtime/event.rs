//! The loop's input vocabulary: what a keypress or mouse action means, and in
//! which mode.
//!
//! Kept apart from the loop that consumes these so the key bindings in
//! [`super::terminal`] and the handlers in [`super::event_loop`] can be read
//! against one shared definition.
//!
//! Bindings follow vim, and AstroNvim where it has a convention: `j`/`k` and
//! arrows move, `gg`/`G` jump to the ends, `<C-d>`/`<C-u>` scroll a half page,
//! `:` opens the command line, and `<Space>` is the leader that opens a
//! which-key popup.

use crate::tui::{
    app::{TuiApp, View},
    ui::HitTarget,
};

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
    /// `g` — the prefix half of vim's `gg`; only the second one jumps.
    GPrefix,
    /// Jump to the top of the active list (`gg`, `Home`).
    NavFirst,
    /// `G` — jump to the bottom of the active list.
    NavLast,
    /// `<C-d>` / `<C-u>` — half-page scroll, positive being downward.
    NavHalfPage(i32),
    /// Commits a field edit, applies a theme selection, or submits the
    /// palette line, depending on which view is open.
    Confirm,
    /// A printable character typed into the palette or an editing field.
    PaletteChar(char),
    PaletteBackspace,
    /// `<C-w>` — delete the word before the cursor in a text field.
    DeleteWordBefore,
    /// `<C-u>` — clear the whole line in a text field.
    ClearLine,
    /// `<Space>` on the dashboard: arm the leader and show which-key.
    ToggleLeader,
    /// The second key of a leader sequence.
    LeaderKey(char),
    /// A left click at a terminal cell, resolved against the renderer's hit
    /// map by the event loop.
    MouseClick(u16, u16),
    Ignored,
}

/// What a raw keypress means depends on which view is open and whether a
/// config-editor field is mid-edit — `read_event` needs this to decide, for
/// example, whether `Char('t')` types the letter "t" into a URL field or
/// toggles the theme picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputMode {
    /// Dashboard: single-key shortcuts (`q`, `r`, `+`/`-`, `:`, `t`, `c`) and
    /// the `<Space>` leader.
    Normal,
    /// The which-key popup is open and the next key resolves the sequence.
    Leader,
    /// A modal view (theme picker / config editor) is open but no text
    /// field is being edited: vim motions navigate, Enter acts, Esc closes.
    ModalNav,
    /// A text field (palette input, or a config-editor field mid-edit) is
    /// capturing every printable character.
    TextEntry,
}

pub(super) fn input_mode(app: &TuiApp) -> InputMode {
    // The leader popup outranks the dashboard's own shortcuts: once it is
    // armed, the next key belongs to the sequence.
    if app.leader_pending && app.view == View::Dashboard {
        return InputMode::Leader;
    }

    match app.view {
        View::Dashboard => InputMode::Normal,
        View::CommandPalette => InputMode::TextEntry,
        View::ConfigEditor if app.config_editor_editing.is_some() => InputMode::TextEntry,
        View::ConfigEditor | View::ThemeSettings => InputMode::ModalNav,
    }
}

/// Applies a resolved mouse click. Returns whether anything changed, so the
/// loop only repaints when a click actually landed on something.
pub(super) fn apply_hit(app: &mut TuiApp, target: HitTarget) -> bool {
    match target {
        HitTarget::ThemeRow(index) => {
            app.select_theme_index(index);
            true
        }
        HitTarget::ConfigRow(index) => {
            app.select_config_field_index(index);
            true
        }
    }
}
