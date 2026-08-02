//! The theme picker's state machine.
//!
//! Moving the cursor applies the theme immediately so the whole UI is a live
//! preview. That is why closing must restore `theme_preview_origin`: without
//! it, cancelling would silently leave the previewed theme applied.

use crate::{config, tui::theme};

use super::{TuiApp, View};

impl TuiApp {
    pub fn open_theme_settings(&mut self) {
        self.theme_settings_return = self.view;
        self.theme_preview_origin = Some(self.theme);
        self.settings_cursor = self.theme.index();
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
        let half = (theme::ALL.len() / 2).max(1) as i32;
        self.move_theme_cursor(direction * half);
    }

    /// Selects a theme by index, e.g. from a mouse click on its row.
    pub fn select_theme_index(&mut self, index: usize) {
        self.set_theme_cursor(index);
    }

    fn move_theme_cursor(&mut self, delta: i32) {
        let last = theme::ALL.len().saturating_sub(1) as i32;
        let next = (self.settings_cursor as i32 + delta).clamp(0, last);
        self.set_theme_cursor(next as usize);
    }

    fn set_theme_cursor(&mut self, index: usize) {
        // Every cursor move previews the theme immediately; `close_theme_settings`
        // is what puts the original back if the user cancels.
        self.settings_cursor = index.min(theme::ALL.len().saturating_sub(1));
        self.theme = theme::ALL[self.settings_cursor];
    }

    pub fn confirm_theme_settings(&mut self) {
        let chosen = theme::ALL[self.settings_cursor];
        self.theme = chosen;
        self.theme_preview_origin = None;
        self.view = self.theme_settings_return;
        self.persist_theme(chosen.id);
    }

    pub(super) fn persist_theme(&self, id: &str) {
        if let Some(path) = &self.config_path {
            let _ = config::set_theme(path, id);
        }
    }
}
