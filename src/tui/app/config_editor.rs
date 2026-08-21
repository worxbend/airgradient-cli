//! The config editor's state machine.
//!
//! Edits accumulate in `config_draft` and only reach disk on "Save & Close",
//! so Esc can discard the whole form. Fields are committed to the draft one
//! at a time as each edit is confirmed.

use super::ListCursor;
use crate::{
    config::{self, Config},
    device_url::DeviceUrl,
    refresh_interval::RefreshInterval,
    tui::theme,
};

use super::{ConfigField, TuiApp, View, palette::delete_word_before};

impl TuiApp {
    /// Loads the draft from disk and opens the editor.
    ///
    /// The read is tolerant ([`config::read_display_config`]), not strict. A
    /// strict read fails on the *first* malformed field, which used to leave
    /// this building a draft out of live session state plus `Config::default()`
    /// — silently resetting `notifications_enabled` and `start_minimized` to
    /// their defaults, and then writing those defaults over the desktop app's
    /// real values on Save & Close. Falling back per field instead means one
    /// bad value costs that one value, and says so in the error line.
    pub fn open_config_editor(&mut self) {
        // Fingerprint before reading, so the window a save checks covers the
        // read as well as however long the user spends editing.
        self.config_draft_version = self.config_path.as_deref().map(config::config_version);

        let (draft, warnings) = self
            .config_path
            .as_deref()
            .map(config::read_display_config)
            .transpose()
            .ok()
            .flatten()
            .map(|display| (display.config, display.warnings))
            .unwrap_or_else(|| (self.config_from_session(), Vec::new()));

        self.config_draft = draft;
        self.config_editor_cursor.set(0, ConfigField::ALL.len());
        self.config_editor_editing = None;
        self.config_editor_error = if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("; "))
        };
        self.view = View::ConfigEditor;
    }

    /// The draft to edit when there is no config file to read — or when reading
    /// it failed outright (unreadable path, non-object top level), as opposed to
    /// individual fields being malformed.
    fn config_from_session(&self) -> Config {
        Config {
            server_url: self.configured_url.as_ref().map(ToString::to_string),
            refresh_interval: self.refresh_interval,
            theme: self.theme.id.to_string(),
            ..Config::default()
        }
    }

    pub fn close_config_editor(&mut self) {
        self.config_editor_editing = None;
        self.config_editor_error = None;
        self.view = View::Dashboard;
    }

    /// The field the cursor is on.
    ///
    /// Every writer clamps the cursor, so the fallback is unreachable — but it
    /// is a fallback rather than an index panic, because "the config editor
    /// opened on the first field" is a recoverable answer and a crash in a
    /// full-screen TUI is not.
    pub fn config_editor_field(&self) -> ConfigField {
        ConfigField::ALL
            .get(self.config_field_cursor())
            .copied()
            .unwrap_or(ConfigField::ServerUrl)
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
        let half = ListCursor::half_page(ConfigField::ALL.len());
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
        self.config_editor_cursor
            .move_by(delta, ConfigField::ALL.len());
    }

    fn set_config_cursor(&mut self, index: usize) {
        self.config_editor_cursor.set(index, ConfigField::ALL.len());
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
    ///
    /// A rejected value keeps the buffer open. Taking it unconditionally, as
    /// this used to, threw away what the user typed the moment it failed to
    /// validate: a URL with one character wrong came back as an error message
    /// and an empty field, and had to be retyped from the start.
    pub fn config_editor_confirm(&mut self) {
        if let Some(buffer) = self.config_editor_editing.clone() {
            if self.config_editor_commit_edit(&buffer) {
                self.config_editor_editing = None;
            }
            return;
        }

        match self.config_editor_field() {
            ConfigField::ServerUrl => {
                self.config_editor_editing =
                    Some(self.config_draft.server_url.clone().unwrap_or_default());
            }
            ConfigField::RefreshInterval => {
                self.config_editor_editing =
                    Some(self.config_draft.refresh_interval.as_secs().to_string());
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

    /// Applies `buffer` to the selected field, returning whether it was
    /// accepted. A `false` return leaves the caller holding the buffer so the
    /// user can correct it rather than retype it.
    fn config_editor_commit_edit(&mut self, buffer: &str) -> bool {
        match self.config_editor_field() {
            ConfigField::ServerUrl => {
                // An empty field means "no device configured", which is a valid
                // value rather than a rejected one.
                if buffer.trim().is_empty() {
                    self.config_draft.server_url = None;
                    self.config_editor_error = None;
                    return true;
                }
                match DeviceUrl::parse(buffer) {
                    Ok(normalized) => {
                        self.config_draft.server_url = Some(normalized.to_string());
                        self.config_editor_error = None;
                        true
                    }
                    Err(error) => {
                        self.config_editor_error = Some(error.to_string());
                        false
                    }
                }
            }
            ConfigField::RefreshInterval => match buffer.parse::<u64>() {
                Ok(seconds) => match RefreshInterval::from_secs(seconds) {
                    Ok(interval) => {
                        self.config_draft.refresh_interval = interval;
                        self.config_editor_error = None;
                        true
                    }
                    Err(error) => {
                        self.config_editor_error = Some(error.to_string());
                        false
                    }
                },
                Err(_) => {
                    self.config_editor_error =
                        Some("refresh interval must be a whole number of seconds".to_string());
                    false
                }
            },
            // Not text fields — reachable only if a buffer was somehow opened
            // on one, in which case discarding it is the harmless answer.
            ConfigField::NotificationsEnabled
            | ConfigField::StartMinimized
            | ConfigField::Theme
            | ConfigField::SaveAndClose => true,
        }
    }

    fn config_editor_save(&mut self) {
        if let Some(path) = self.config_path.clone() {
            // The draft was read when the editor opened and the user may have
            // been editing for minutes since. The desktop app shares this file,
            // so the save is refused if it changed underneath — otherwise their
            // edit would vanish with no clue but a setting that reverted.
            let expected = self
                .config_draft_version
                .clone()
                .unwrap_or_else(|| config::config_version(&path));
            match config::write_config_if_unchanged(&path, &self.config_draft, &expected) {
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
        // A draft that does not parse leaves the live address alone: the editor
        // is still open on the text that failed, and dropping the working
        // address would take the dashboard down with a typo.
        if let Ok(url) = self.config_draft.device_url() {
            self.configured_url = url;
        }
        self.refresh_interval = self.config_draft.refresh_interval;
        self.theme = theme::Theme::by_id(&self.config_draft.theme);
    }
}
