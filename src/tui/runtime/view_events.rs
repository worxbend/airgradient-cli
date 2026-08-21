//! What each event does to the model, one handler per view.
//!
//! Pure state transitions: no clock, no network, no terminal. The loop that
//! decides *when* to call these — and that owns the fetch, the refresh
//! schedule and the screen — lives in [`super::event_loop`].
//!
//! Keeping the two apart is what lets the view bindings be read and changed
//! without stepping through async code, and what lets them be reasoned about
//! as plain functions of `(event, app)`.

use crate::tui::{
    app::{PaletteOutcome, TuiApp, View},
    runtime::event::RuntimeEvent,
};

/// What the loop should do after applying an event to the app model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EventOutcome {
    /// App state changed — redraw and keep going.
    Redraw,
    /// The key was not bound in this view; nothing to repaint.
    Unchanged,
    Quit,
}

/// Applies the events that only move view state around — no clock, no network.
///
/// Events that mean the same thing everywhere are handled once, up front; the
/// rest are dispatched to the one view that binds them. Organising it this way
/// round matters: this used to be a match on the *event* with a nested match on
/// `app.view` inside eleven of its arms, so adding a view meant finding and
/// editing all eleven.
pub(super) fn apply_view_event(event: RuntimeEvent, app: &mut TuiApp) -> EventOutcome {
    // Any other key breaks a half-typed `g` prefix, exactly as in vim.
    let had_pending_g = std::mem::take(&mut app.pending_g);

    if let Some(outcome) = apply_global_event(event, app, had_pending_g) {
        return outcome;
    }

    match app.view {
        View::ThemeSettings => apply_theme_settings_event(event, app),
        View::ConfigEditor => apply_config_editor_event(event, app),
        View::CommandPalette => apply_command_palette_event(event, app),
        View::History => apply_history_event(event, app),
        View::Dashboard => EventOutcome::Unchanged,
    }
}

/// The events that mean the same thing in every view, plus the ones no view
/// binds. `None` means "not a global event" and hands over to the view.
fn apply_global_event(
    event: RuntimeEvent,
    app: &mut TuiApp,
    had_pending_g: bool,
) -> Option<EventOutcome> {
    match event {
        RuntimeEvent::Quit => Some(EventOutcome::Quit),
        RuntimeEvent::OpenPalette => {
            app.open_command_palette();
            Some(EventOutcome::Redraw)
        }
        RuntimeEvent::ToggleThemeSettings => {
            if app.view == View::ThemeSettings {
                app.close_theme_settings();
            } else {
                app.open_theme_settings();
            }
            Some(EventOutcome::Redraw)
        }
        RuntimeEvent::ToggleConfigEditor => {
            if app.view == View::ConfigEditor {
                app.close_config_editor();
            } else {
                app.open_config_editor();
            }
            Some(EventOutcome::Redraw)
        }
        RuntimeEvent::ToggleHistory => {
            if app.view == View::History {
                app.close_history();
            } else {
                app.open_history();
            }
            Some(EventOutcome::Redraw)
        }
        RuntimeEvent::ToggleLeader => {
            app.toggle_leader();
            Some(EventOutcome::Redraw)
        }
        RuntimeEvent::Escape if app.leader_pending => {
            app.cancel_leader();
            Some(EventOutcome::Redraw)
        }
        RuntimeEvent::Escape => {
            match app.view {
                View::ThemeSettings => app.close_theme_settings(),
                View::ConfigEditor if app.config_editor_editing.is_some() => {
                    app.config_editor_cancel_edit();
                }
                View::ConfigEditor => app.close_config_editor(),
                View::CommandPalette => app.close_command_palette(),
                View::History => app.close_history(),
                View::Dashboard => {}
            }
            Some(EventOutcome::Redraw)
        }
        // The first `g` of `gg` only arms the prefix; the second reaches the
        // view below as a jump-to-first.
        RuntimeEvent::GPrefix if !had_pending_g => {
            app.pending_g = true;
            Some(EventOutcome::Unchanged)
        }
        RuntimeEvent::Ignored => Some(EventOutcome::Unchanged),
        // Handled by `LoopContext::handle_event`, which needs the clock, the
        // scheduler, and the terminal's hit map.
        RuntimeEvent::Refresh
        | RuntimeEvent::IncreaseRefreshInterval
        | RuntimeEvent::DecreaseRefreshInterval
        | RuntimeEvent::LeaderKey(_)
        | RuntimeEvent::MouseClick(_, _) => Some(EventOutcome::Unchanged),
        _ => None,
    }
}

fn apply_theme_settings_event(event: RuntimeEvent, app: &mut TuiApp) -> EventOutcome {
    match event {
        RuntimeEvent::NavUp => app.theme_cursor_up(),
        RuntimeEvent::NavDown => app.theme_cursor_down(),
        // A `GPrefix` reaching a view is the second `g` of `gg`.
        RuntimeEvent::NavFirst | RuntimeEvent::GPrefix => app.theme_cursor_first(),
        RuntimeEvent::NavLast => app.theme_cursor_last(),
        RuntimeEvent::NavHalfPage(direction) => app.theme_cursor_half_page(direction),
        RuntimeEvent::Confirm => app.confirm_theme_settings(),
        _ => return EventOutcome::Unchanged,
    }

    EventOutcome::Redraw
}

fn apply_config_editor_event(event: RuntimeEvent, app: &mut TuiApp) -> EventOutcome {
    match event {
        RuntimeEvent::NavUp => app.config_editor_nav_up(),
        RuntimeEvent::NavDown => app.config_editor_nav_down(),
        RuntimeEvent::NavFirst | RuntimeEvent::GPrefix => app.config_editor_nav_first(),
        RuntimeEvent::NavLast => app.config_editor_nav_last(),
        RuntimeEvent::NavHalfPage(direction) => app.config_editor_half_page(direction),
        RuntimeEvent::Confirm => app.config_editor_confirm(),
        RuntimeEvent::PaletteChar(c) => app.config_editor_push_char(c),
        RuntimeEvent::PaletteBackspace => app.config_editor_backspace(),
        RuntimeEvent::DeleteWordBefore => app.config_editor_delete_word(),
        RuntimeEvent::ClearLine => app.config_editor_clear_line(),
        _ => return EventOutcome::Unchanged,
    }

    EventOutcome::Redraw
}

/// The chart binds the same motions as the other modal views; here they move
/// between metrics rather than between rows of a list.
fn apply_history_event(event: RuntimeEvent, app: &mut TuiApp) -> EventOutcome {
    match event {
        RuntimeEvent::NavUp => app.history_previous_metric(),
        RuntimeEvent::NavDown => app.history_next_metric(),
        RuntimeEvent::NavFirst | RuntimeEvent::GPrefix => app.history_first_metric(),
        RuntimeEvent::NavLast => app.history_last_metric(),
        RuntimeEvent::NavHalfPage(direction) => app.history_half_page(direction),
        _ => return EventOutcome::Unchanged,
    }

    EventOutcome::Redraw
}

fn apply_command_palette_event(event: RuntimeEvent, app: &mut TuiApp) -> EventOutcome {
    match event {
        RuntimeEvent::Confirm => {
            if app.palette_submit() == PaletteOutcome::Quit {
                return EventOutcome::Quit;
            }
        }
        RuntimeEvent::PaletteChar(c) => app.palette_push_char(c),
        RuntimeEvent::PaletteBackspace => app.palette_backspace(),
        RuntimeEvent::DeleteWordBefore => app.palette_delete_word(),
        RuntimeEvent::ClearLine => app.palette_clear_line(),
        _ => return EventOutcome::Unchanged,
    }

    EventOutcome::Redraw
}
