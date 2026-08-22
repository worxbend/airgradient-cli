//! The `:` command palette's state machine.
//!
//! Each verb applies to the live session first and is then persisted
//! best-effort: a config file that cannot be written must not undo a change
//! the user can already see on screen.

use crate::{
    config,
    device_url::DeviceUrl,
    refresh_interval::RefreshInterval,
    tui::{
        command::{self, PaletteCommand},
        theme::Theme,
    },
};

use super::{PaletteOutcome, TuiApp, View};

/// Whether a palette message reports something that worked or something that
/// did not.
///
/// The two are drawn differently -- an error in the theme's error colour, a
/// success in its success colour -- and the difference used to travel as a
/// bare `bool` alongside the text. At a call site `Some((message, true))` says
/// nothing about which way round it is, and transposing it produces a green
/// failure or a red success that still compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMessageKind {
    Success,
    Error,
}

/// What the palette shows after running a command.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteMessage {
    pub text: String,
    pub kind: PaletteMessageKind,
}

impl PaletteMessage {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: PaletteMessageKind::Success,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: PaletteMessageKind::Error,
        }
    }
}

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
                match DeviceUrl::parse(&url) {
                    Ok(normalized) => {
                        self.configured_url = Some(normalized.clone());
                        self.persist_url(normalized.as_str());
                        self.palette_message = Some(PaletteMessage::success(format!("url set: {normalized}")));
                    }
                    Err(error) => {
                        self.palette_message = Some(PaletteMessage::error(format!("invalid url: {error}")));
                    }
                }
                PaletteOutcome::Continue
            }
            Ok(PaletteCommand::SetRefresh(seconds)) => {
                match RefreshInterval::from_secs(seconds) {
                    Ok(interval) => {
                        self.refresh_interval = interval;
                        self.persist_refresh(seconds);
                        self.palette_message = Some(PaletteMessage::success(format!(
                            "refresh interval set: {interval}"
                        )));
                    }
                    Err(error) => {
                        self.palette_message = Some(PaletteMessage::error(error.to_string()));
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
                        self.palette_message = Some(PaletteMessage::success(format!(
                            "theme set: {}",
                            resolved.label
                        )));
                    }
                    None => {
                        self.palette_message = Some(PaletteMessage::error(format!("unknown theme: {id}")));
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
                self.palette_message = Some(PaletteMessage::error(message));
                PaletteOutcome::Continue
            }
        }
    }

    /// Writes the session's URL, refresh interval, and theme to the config
    /// file, returning the message to show for it.
    ///
    /// Every other palette verb persists as a side effect of the change it
    /// makes, so `:save` matters mainly when there is nothing to persist to
    /// yet -- and saying "config saved" when no file is configured would be a
    /// lie, which is why that case reports instead.
    fn save_session(&self) -> PaletteMessage {
        let Some(path) = &self.config_path else {
            return PaletteMessage::error("no config file to save to");
        };

        let url = self.configured_url.as_ref().map(DeviceUrl::as_str);
        match config::set_session(path, url, self.refresh_interval, self.theme.id) {
            Ok(_) => PaletteMessage::success(format!("config saved: {}", path.display())),
            Err(error) => PaletteMessage::error(format!("could not save config: {error}")),
        }
    }

    /// Any repairs the write had to make are dropped on purpose: the TUI owns
    /// the alternate screen, so there is nowhere to print them to, and the
    /// config editor shows the repaired values anyway.
    fn persist_url(&self, url: &str) {
        if let Some(path) = &self.config_path {
            let _ = config::set_url(path, url);
        }
    }

    fn persist_refresh(&self, seconds: u64) {
        if let Some(path) = &self.config_path {
            // Repairs dropped on purpose -- see `persist_url`.
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
