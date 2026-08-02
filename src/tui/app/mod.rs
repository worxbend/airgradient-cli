//! The TUI's app model: every piece of state a frame is rendered from, and
//! the transitions the event loop applies to it.
//!
//! This module owns the state itself plus the fetch lifecycle and refresh
//! interval. The interactive modes each own their own transitions:
//! [`palette`], [`theme_settings`], [`config_editor`], and [`leader`].
//!
//! Nothing here does I/O on the render path — persisting a change is
//! best-effort and never blocks the state update the user just made.

mod config_editor;
mod leader;
mod palette;
mod theme_settings;

#[cfg(test)]
mod tests;

use std::{
    collections::VecDeque,
    fmt,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use url::Url;

pub use leader::{LEADER_BINDINGS, LeaderAction, LeaderBinding};

use crate::{
    config::{Config, MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS},
    sensors::{Metric, SensorSnapshot, metrics},
    tui::theme::Theme,
};

const REFRESH_STEP: Duration = Duration::from_secs(5);
const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);
const MAX_REFRESH_INTERVAL: Duration = Duration::from_secs(MAX_REFRESH_INTERVAL_SECS);
const MAX_READING_HISTORY: usize = 48;

/// Which full-screen (or overlay) mode the TUI is currently showing. Gates
/// both rendering (`ui::draw`) and which keys are live (`runtime::run_loop`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    ThemeSettings,
    ConfigEditor,
    CommandPalette,
}

/// The config-editor's field list, in display/navigation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigField {
    ServerUrl,
    RefreshInterval,
    NotificationsEnabled,
    StartMinimized,
    Theme,
    SaveAndClose,
}

impl ConfigField {
    pub const ALL: [ConfigField; 6] = [
        ConfigField::ServerUrl,
        ConfigField::RefreshInterval,
        ConfigField::NotificationsEnabled,
        ConfigField::StartMinimized,
        ConfigField::Theme,
        ConfigField::SaveAndClose,
    ];
}

/// Result of submitting a line in the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteOutcome {
    Continue,
    Quit,
}

#[derive(Debug, Clone)]
pub struct TuiApp {
    pub current_snapshot: Option<SensorSnapshot>,
    pub previous_successful_snapshot: Option<SensorSnapshot>,
    reading_history: VecDeque<SensorSnapshot>,
    pub last_fetch_duration: Option<Duration>,
    pub last_success_at: Option<SystemTime>,
    pub current_error: Option<String>,
    pub configured_url: Option<Url>,
    pub refresh_interval: Duration,
    pub is_fetching: bool,

    /// Active color theme, applied throughout `ui.rs`.
    pub theme: Theme,
    /// Where to persist theme/config changes made in the TUI. `None` in
    /// tests that build `TuiApp` directly; set by `runtime::run` for real
    /// sessions.
    pub config_path: Option<PathBuf>,
    /// Which screen is currently showing.
    pub view: View,
    /// Which `View` to return to when the theme picker (opened either
    /// directly or from inside the config editor) closes.
    pub theme_settings_return: View,

    /// Cursor into `theme::ALL` while the theme picker is open.
    pub settings_cursor: usize,
    /// Theme active before the picker opened, restored on Esc-cancel.
    pub theme_preview_origin: Option<Theme>,

    /// Text currently typed into the `:` command palette.
    pub palette_input: String,
    /// Last palette result (message, is_error), shown until the next submit.
    pub palette_message: Option<(String, bool)>,

    /// Working copy of `Config` edited by the config editor, persisted only
    /// on "Save & Close" — everything else in this view previews live but
    /// discards on Esc.
    pub config_draft: Config,
    /// Cursor into `ConfigField::ALL`.
    pub config_editor_cursor: usize,
    /// `Some(buffer)` while the selected text field is being edited.
    pub config_editor_editing: Option<String>,
    pub config_editor_error: Option<String>,

    /// `Some(frame_no)` while the startup splash animation is showing;
    /// checked first thing by `ui::draw`. Only ever set by the production
    /// runtime path — stays `None` in unit/render tests.
    pub splash_frame: Option<u64>,

    /// True while `<Space>` has been pressed and the which-key popup is
    /// waiting for the second key of the sequence, mirroring AstroNvim's
    /// leader behavior.
    pub leader_pending: bool,

    /// First key of a pending `g`-prefixed motion (vim's `gg`). Cleared as
    /// soon as the next key resolves it, so `g` followed by anything else is
    /// simply discarded rather than remembered.
    pub pending_g: bool,
}

impl TuiApp {
    pub fn new(configured_url: Option<Url>, refresh_interval: Duration) -> Self {
        Self {
            current_snapshot: None,
            previous_successful_snapshot: None,
            reading_history: VecDeque::new(),
            last_fetch_duration: None,
            last_success_at: None,
            current_error: None,
            configured_url,
            refresh_interval: clamp_refresh_interval(refresh_interval),
            is_fetching: false,

            theme: Theme::default_theme(),
            config_path: None,
            view: View::Dashboard,
            theme_settings_return: View::Dashboard,

            settings_cursor: 0,
            theme_preview_origin: None,

            palette_input: String::new(),
            palette_message: None,

            config_draft: Config::default(),
            config_editor_cursor: 0,
            config_editor_editing: None,
            config_editor_error: None,

            splash_frame: None,
            leader_pending: false,
            pending_g: false,
        }
    }

    pub fn begin_fetch(&mut self) {
        self.is_fetching = self.configured_url.is_some();
    }

    pub fn finish_fetch_success(
        &mut self,
        snapshot: SensorSnapshot,
        fetch_duration: Duration,
        success_at: SystemTime,
    ) {
        self.is_fetching = false;
        self.remember_successful_snapshot(snapshot.clone());
        self.previous_successful_snapshot = self.current_snapshot.replace(snapshot);
        self.last_fetch_duration = Some(fetch_duration);
        self.last_success_at = Some(success_at);
        self.current_error = None;
    }

    pub fn finish_fetch_failure(&mut self, error: impl fmt::Display, fetch_duration: Duration) {
        self.is_fetching = false;
        self.last_fetch_duration = Some(fetch_duration);
        self.current_error = Some(error.to_string());
    }

    pub fn increase_refresh_interval(&mut self) {
        self.refresh_interval = self
            .refresh_interval
            .saturating_add(REFRESH_STEP)
            .min(MAX_REFRESH_INTERVAL);
    }

    pub fn decrease_refresh_interval(&mut self) {
        self.refresh_interval = self
            .refresh_interval
            .saturating_sub(REFRESH_STEP)
            .max(MIN_REFRESH_INTERVAL);
    }

    pub fn trend_baseline(&self) -> Option<&SensorSnapshot> {
        self.previous_successful_snapshot.as_ref()
    }

    pub fn metrics(&self) -> Vec<Metric> {
        self.current_snapshot
            .as_ref()
            .map(|snapshot| metrics(snapshot, self.trend_baseline()))
            .unwrap_or_default()
    }

    pub fn successful_snapshots(&self) -> impl Iterator<Item = &SensorSnapshot> {
        self.reading_history.iter()
    }

    fn remember_successful_snapshot(&mut self, snapshot: SensorSnapshot) {
        self.reading_history.push_back(snapshot);
        while self.reading_history.len() > MAX_READING_HISTORY {
            self.reading_history.pop_front();
        }
    }
}

pub(super) fn clamp_refresh_interval(refresh_interval: Duration) -> Duration {
    refresh_interval.clamp(MIN_REFRESH_INTERVAL, MAX_REFRESH_INTERVAL)
}
