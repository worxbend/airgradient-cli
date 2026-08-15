//! The `:` command palette's state machine.
//!
//! Each verb applies to the live session first and is then persisted
//! best-effort: a config file that cannot be written must not undo a change
//! the user can already see on screen.

use std::time::Duration;

use url::Url;

use crate::{
    config, device,
    tui::{
        command::{self, PaletteCommand},
        theme::Theme,
    },
};

use super::{PaletteOutcome, TuiApp, View};

impl TuiApp {
    pub fn open_command_palette(&mut self) {
        self.palette_input.clear();
        self.palette_message = None;
        self.view = View::CommandPalette;
    }

    pub fn close_command_palette(&mut self) {
        self.palette_input.clear();
        self.view = View::Dashboard;
    }

    pub fn palette_push_char(&mut self, c: char) {
        self.palette_input.push(c);
    }

    pub fn palette_backspace(&mut self) {
        self.palette_input.pop();
    }

    /// `<C-w>` — delete the word before the cursor, vim/readline style.
    pub fn palette_delete_word(&mut self) {
        delete_word_before(&mut self.palette_input);
    }

    /// `<C-u>` — clear the whole line.
    pub fn palette_clear_line(&mut self) {
        self.palette_input.clear();
    }

    pub fn palette_submit(&mut self) -> PaletteOutcome {
        let input = std::mem::take(&mut self.palette_input);
        self.view = View::Dashboard;

        match command::parse(&input) {
            Ok(PaletteCommand::SetUrl(url)) => {
                match device::normalize_base_url(&url) {
                    Ok(normalized) => {
                        self.configured_url = Some(normalized.clone());
                        self.persist_url(normalized.as_str());
                        self.palette_message = Some((format!("url set: {normalized}"), false));
                    }
                    Err(error) => {
                        self.palette_message = Some((format!("invalid url: {error}"), true));
                    }
                }
                PaletteOutcome::Continue
            }
            Ok(PaletteCommand::SetRefresh(seconds)) => {
                match config::validate_refresh_interval(seconds) {
                    Ok(()) => {
                        self.refresh_interval = Duration::from_secs(seconds);
                        self.persist_refresh(seconds);
                        self.palette_message =
                            Some((format!("refresh interval set: {seconds}s"), false));
                    }
                    Err(error) => {
                        self.palette_message = Some((error.to_string(), true));
                    }
                }
                PaletteOutcome::Continue
            }
            Ok(PaletteCommand::SetTheme(id)) => {
                // Deliberately `find` rather than `by_id`: the latter turns an
                // unknown id into the default theme, which here would answer a
                // typo like `:theme drakula` with a green "theme set:
                // AirGradient" and overwrite whatever theme was stored.
                match Theme::find(&id) {
                    Some(resolved) => {
                        self.theme = resolved;
                        self.persist_theme(resolved.id);
                        self.palette_message =
                            Some((format!("theme set: {}", resolved.label), false));
                    }
                    None => {
                        self.palette_message = Some((format!("unknown theme: {id}"), true));
                    }
                }
                PaletteOutcome::Continue
            }
            Ok(PaletteCommand::OpenConfig) => {
                self.open_config_editor();
                PaletteOutcome::Continue
            }
            Ok(PaletteCommand::OpenThemes) => {
                self.open_theme_settings();
                PaletteOutcome::Continue
            }
            Ok(PaletteCommand::Save) => {
                self.palette_message = Some(self.save_session());
                PaletteOutcome::Continue
            }
            Ok(PaletteCommand::Quit) => PaletteOutcome::Quit,
            Err(message) => {
                self.palette_message = Some((message, true));
                PaletteOutcome::Continue
            }
        }
    }

    /// Writes the session's URL, refresh interval, and theme to the config
    /// file, returning the message to show and whether it is an error.
    ///
    /// Every other palette verb persists as a side effect of the change it
    /// makes, so `:save` matters mainly when there is nothing to persist to
    /// yet -- and saying "config saved" when no file is configured would be a
    /// lie, which is why that case reports instead.
    fn save_session(&self) -> (String, bool) {
        let Some(path) = &self.config_path else {
            return ("no config file to save to".to_string(), true);
        };

        let url = self.configured_url.as_ref().map(Url::as_str);
        match config::set_session(path, url, self.refresh_interval.as_secs(), self.theme.id) {
            Ok(_) => (format!("config saved: {}", path.display()), false),
            Err(error) => (format!("could not save config: {error}"), true),
        }
    }

    fn persist_url(&self, url: &str) {
        if let Some(path) = &self.config_path {
            let _ = config::set_url(path, url);
        }
    }

    fn persist_refresh(&self, seconds: u64) {
        if let Some(path) = &self.config_path {
            let _ = config::set_refresh_interval(path, seconds);
        }
    }
}

/// Deletes the word before the end of `buffer`, matching `<C-w>` in vim's
/// insert mode and in readline: trailing whitespace goes first, then the run
/// of non-whitespace before it. An empty buffer is left alone.
pub(super) fn delete_word_before(buffer: &mut String) {
    while buffer.ends_with(char::is_whitespace) {
        buffer.pop();
    }
    while !buffer.is_empty() && !buffer.ends_with(char::is_whitespace) {
        buffer.pop();
    }
}
