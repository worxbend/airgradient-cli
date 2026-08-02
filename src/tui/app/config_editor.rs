//! The config editor's state machine.
//!
//! Edits accumulate in `config_draft` and only reach disk on "Save & Close",
//! so Esc can discard the whole form. Fields are committed to the draft one
//! at a time as each edit is confirmed.

use std::time::Duration;

use crate::{
    config::{self, Config, MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS},
    device,
};

use super::{ConfigField, TuiApp, View, clamp_refresh_interval, palette::delete_word_before};

impl TuiApp {
    pub fn open_config_editor(&mut self) {
        self.config_draft = self
            .config_path
            .as_deref()
            .and_then(|path| config::read_config(path).ok())
            .unwrap_or_else(|| Config {
                server_url: self.configured_url.as_ref().map(ToString::to_string),
                refresh_interval_secs: self.refresh_interval.as_secs(),
                theme: self.theme.id.to_string(),
                ..Config::default()
            });
        self.config_editor_cursor = 0;
        self.config_editor_editing = None;
        self.config_editor_error = None;
        self.view = View::ConfigEditor;
    }

    pub fn close_config_editor(&mut self) {
        self.config_editor_editing = None;
        self.config_editor_error = None;
        self.view = View::Dashboard;
    }

    pub fn config_editor_field(&self) -> ConfigField {
        ConfigField::ALL[self.config_editor_cursor]
    }

    pub fn config_editor_nav_up(&mut self) {
        self.move_config_cursor(-1);
    }

    pub fn config_editor_nav_down(&mut self) {
        self.move_config_cursor(1);
    }

    /// `gg` — jump to the first field.
    pub fn config_editor_nav_first(&mut self) {
        self.set_config_cursor(0);
    }

    /// `G` — jump to the last row, which is "Save & Close".
    pub fn config_editor_nav_last(&mut self) {
        self.set_config_cursor(ConfigField::ALL.len().saturating_sub(1));
    }

    /// `<C-d>` / `<C-u>` — half-page scroll over the field list.
    pub fn config_editor_half_page(&mut self, direction: i32) {
        let half = (ConfigField::ALL.len() / 2).max(1) as i32;
        self.move_config_cursor(direction * half);
    }

    /// Moves the cursor to a field, e.g. from a mouse click on its row.
    /// Ignored mid-edit so a stray click cannot silently retarget the buffer
    /// the user is typing into.
    pub fn select_config_field_index(&mut self, index: usize) {
        if self.config_editor_editing.is_some() {
            return;
        }
        self.set_config_cursor(index);
    }

    fn move_config_cursor(&mut self, delta: i32) {
        let last = ConfigField::ALL.len().saturating_sub(1) as i32;
        let next = (self.config_editor_cursor as i32 + delta).clamp(0, last);
        self.set_config_cursor(next as usize);
    }

    fn set_config_cursor(&mut self, index: usize) {
        self.config_editor_cursor = index.min(ConfigField::ALL.len().saturating_sub(1));
    }

    pub fn config_editor_push_char(&mut self, c: char) {
        if let Some(buffer) = self.config_editor_editing.as_mut() {
            buffer.push(c);
        }
    }

    /// `<C-w>` in a config field being edited.
    pub fn config_editor_delete_word(&mut self) {
        if let Some(buffer) = self.config_editor_editing.as_mut() {
            delete_word_before(buffer);
        }
    }

    /// `<C-u>` in a config field being edited.
    pub fn config_editor_clear_line(&mut self) {
        if let Some(buffer) = self.config_editor_editing.as_mut() {
            buffer.clear();
        }
    }

    pub fn config_editor_backspace(&mut self) {
        if let Some(buffer) = self.config_editor_editing.as_mut() {
            buffer.pop();
        }
    }

    pub fn config_editor_cancel_edit(&mut self) {
        self.config_editor_editing = None;
        self.config_editor_error = None;
    }

    /// Handles Enter: commits the in-progress field edit if there is one,
    /// otherwise begins editing/toggles/opens/saves based on the selected
    /// field.
    pub fn config_editor_confirm(&mut self) {
        if let Some(buffer) = self.config_editor_editing.take() {
            self.config_editor_commit_edit(&buffer);
            return;
        }

        match self.config_editor_field() {
            ConfigField::ServerUrl => {
                self.config_editor_editing =
                    Some(self.config_draft.server_url.clone().unwrap_or_default());
            }
            ConfigField::RefreshInterval => {
                self.config_editor_editing =
                    Some(self.config_draft.refresh_interval_secs.to_string());
            }
            ConfigField::NotificationsEnabled => {
                self.config_draft.notifications_enabled = !self.config_draft.notifications_enabled;
            }
            ConfigField::StartMinimized => {
                self.config_draft.start_minimized = !self.config_draft.start_minimized;
            }
            ConfigField::Theme => {
                self.open_theme_settings();
            }
            ConfigField::SaveAndClose => {
                self.config_editor_save();
            }
        }
    }

    fn config_editor_commit_edit(&mut self, buffer: &str) {
        match self.config_editor_field() {
            ConfigField::ServerUrl => {
                if buffer.trim().is_empty() {
                    self.config_draft.server_url = None;
                    self.config_editor_error = None;
                    return;
                }
                match device::normalize_base_url(buffer) {
                    Ok(normalized) => {
                        self.config_draft.server_url = Some(normalized.to_string());
                        self.config_editor_error = None;
                    }
                    Err(error) => {
                        self.config_editor_error = Some(error.to_string());
                    }
                }
            }
            ConfigField::RefreshInterval => match buffer.parse::<u64>() {
                Ok(seconds) if config::validate_refresh_interval(seconds).is_ok() => {
                    self.config_draft.refresh_interval_secs = seconds;
                    self.config_editor_error = None;
                }
                Ok(_) => {
                    self.config_editor_error = Some(format!(
                        "refresh interval must be between {MIN_REFRESH_INTERVAL_SECS} and {MAX_REFRESH_INTERVAL_SECS} seconds"
                    ));
                }
                Err(_) => {
                    self.config_editor_error =
                        Some("refresh interval must be a whole number of seconds".to_string());
                }
            },
            ConfigField::NotificationsEnabled
            | ConfigField::StartMinimized
            | ConfigField::Theme
            | ConfigField::SaveAndClose => {}
        }
    }

    fn config_editor_save(&mut self) {
        if let Some(path) = self.config_path.clone() {
            match config::write_config(&path, &self.config_draft) {
                Ok(()) => {
                    self.apply_draft_to_live_state();
                    self.close_config_editor();
                }
                Err(error) => {
                    self.config_editor_error = Some(error.to_string());
                }
            }
        } else {
            self.apply_draft_to_live_state();
            self.close_config_editor();
        }
    }

    fn apply_draft_to_live_state(&mut self) {
        match self
            .config_draft
            .server_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
        {
            Some(url) => {
                if let Ok(normalized) = device::normalize_base_url(url) {
                    self.configured_url = Some(normalized);
                }
            }
            None => self.configured_url = None,
        }
        self.refresh_interval =
            clamp_refresh_interval(Duration::from_secs(self.config_draft.refresh_interval_secs));
    }
}
