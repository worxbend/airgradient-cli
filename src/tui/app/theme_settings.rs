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
        self.settings_cursor = self.settings_cursor.saturating_sub(1);
        self.theme = theme::ALL[self.settings_cursor];
    }

    pub fn theme_cursor_down(&mut self) {
        let max = theme::ALL.len().saturating_sub(1);
        self.settings_cursor = (self.settings_cursor + 1).min(max);
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
