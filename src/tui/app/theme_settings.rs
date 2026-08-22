//! The theme picker's state machine.
//!
//! Moving the cursor applies the theme immediately so the whole UI is a live
//! preview. That is why closing must restore `theme_preview_origin`: without
//! it, cancelling would silently leave the previewed theme applied.

use super::ListCursor;
use crate::{config, tui::theme};

use super::{TuiApp, View};

impl TuiApp {
    pub fn open_theme_settings(&mut self) {
        self.theme_settings_return = self.view;
        self.theme_preview_origin = Some(self.theme);
        self.settings_cursor
            .set(self.theme.index(), theme::ALL.len());
        self.view = View::ThemeSettings;
    }

    pub fn close_theme_settings(&mut self) {
        if let Some(original) = self.theme_preview_origin.take() {
            self.theme = original;
        }
        self.view = self.theme_settings_return;
    }

    pub fn theme_cursor_up(&mut self) {
        self.move_theme_cursor(-1);
    }

    pub fn theme_cursor_down(&mut self) {
        self.move_theme_cursor(1);
    }

    /// `gg` — jump to the first theme.
    pub fn theme_cursor_first(&mut self) {
        self.set_theme_cursor(0);
    }

    /// `G` — jump to the last theme.
    pub fn theme_cursor_last(&mut self) {
        self.set_theme_cursor(theme::ALL.len().saturating_sub(1));
    }

    /// `<C-d>` / `<C-u>` — vim's half-page scroll. The list is short enough
    /// that a "page" is the whole list, so half of it is the useful step.
    pub fn theme_cursor_half_page(&mut self, direction: i32) {
        let delta = ListCursor::half_page_delta(direction, theme::ALL.len());
        self.move_theme_cursor(delta);
    }

    /// Selects a theme by index, e.g. from a mouse click on its row.
    pub fn select_theme_index(&mut self, index: usize) {
        self.set_theme_cursor(index);
    }

    fn move_theme_cursor(&mut self, delta: i32) {
        self.settings_cursor.move_by(delta, theme::ALL.len());
        self.preview_selected_theme();
    }

    fn set_theme_cursor(&mut self, index: usize) {
        self.settings_cursor.set(index, theme::ALL.len());
        self.preview_selected_theme();
    }

    /// Every cursor move previews the theme immediately;
    /// `close_theme_settings` is what puts the original back on cancel.
    fn preview_selected_theme(&mut self) {
        self.theme = theme::ALL[self.theme_cursor()];
    }

    /// Accepts the highlighted theme and returns to whichever view opened the
    /// picker.
    ///
    /// Where the choice is *stored* depends on that view. Opened from the
    /// dashboard, the picker owns the setting and writes it out immediately.
    /// Opened from the config editor, it does not: that editor collects every
    /// change in `config_draft` and writes the whole thing on Save & Close, so
    /// a write here would be overwritten moments later by a draft still holding
    /// the old theme.
    pub fn confirm_theme_settings(&mut self) {
        let chosen = theme::ALL[self.theme_cursor()];
        self.theme = chosen;
        self.theme_preview_origin = None;
        self.view = self.theme_settings_return;

        if self.view == View::ConfigEditor {
            self.config_draft.theme = chosen.id.to_owned();
        } else {
            self.persist_theme(chosen.id);
        }
    }

    pub(super) fn persist_theme(&self, id: &str) {
        if let Some(path) = &self.config_path {
            // Repairs dropped on purpose -- see `TuiApp::persist_url`.
            let _ = config::set_theme(path, id);
        }
    }
}
