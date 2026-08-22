//! The TUI's app model: every piece of state a frame is rendered from, and
//! the transitions the event loop applies to it.
//!
//! This module owns the state itself plus the fetch lifecycle and refresh
//! interval. The interactive modes each own their own transitions:
//! [`palette`], [`theme_settings`], [`config_editor`], and [`leader`].
//!
//! Nothing here does I/O on the render path — persisting a change is
//! best-effort and never blocks the state update the user just made.

mod alerts;
mod config_editor;
mod history_view;
mod leader;
mod list_cursor;

pub use list_cursor::ListCursor;
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

pub use alerts::{AirAlert, AlertKind};
pub use palette::{PaletteMessage, PaletteMessageKind};
pub use leader::{LEADER_BINDINGS, LeaderAction, LeaderBinding};

use crate::{
    config::{Config, ConfigVersion},
    device_url::DeviceUrl,
    history::{self, Reading},
    refresh_interval::RefreshInterval,
    sensors::{Metrics, SensorSnapshot, metrics, series},
    tui::theme::{self, Theme},
};

/// How many successful readings the dashboard keeps in memory.
///
/// Two things read this buffer: the card sparklines, which slice it down to
/// however many cells they have, and the full-screen chart, which plots all of
/// it. So this is the span the chart covers -- at the default 30-second
/// refresh, a little under half an hour, which is the span over which a room
/// visibly reacts to a window opening or a burner being lit.
///
/// It is also how much of the recorded history a starting session replays, so
/// the chart opens with a shape instead of filling in over the next half hour.
/// Reading more than this would only be discarded on arrival.
pub(crate) const MAX_READING_HISTORY: usize = 48;

/// Which full-screen (or overlay) mode the TUI is currently showing. Gates
/// both rendering (`ui::draw`) and which keys are live (`runtime::run_loop`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    ThemeSettings,
    ConfigEditor,
    CommandPalette,
    /// The full-screen reading chart.
    History,
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
    /// Recent successful readings, oldest first, for the chart. Failed
    /// fetches are not recorded — a gap in the graph would then mean "the
    /// network was down", which is not what the graph is about.
    reading_history: VecDeque<Reading>,
    pub last_fetch_duration: Option<Duration>,
    pub last_success_at: Option<SystemTime>,
    pub current_error: Option<String>,
    pub configured_url: Option<DeviceUrl>,
    pub refresh_interval: RefreshInterval,
    pub is_fetching: bool,

    /// Active color theme, applied throughout `ui.rs`.
    pub theme: Theme,
    /// Where to persist theme/config changes made in the TUI. `None` in
    /// tests that build `TuiApp` directly; set by `runtime::run` for real
    /// sessions.
    pub config_path: Option<PathBuf>,
    /// Where readings are recorded for the chart. `None` in tests, so building
    /// a `TuiApp` directly never touches the filesystem.
    pub history_path: Option<PathBuf>,
    /// Which screen is currently showing.
    pub view: View,
    /// Which `View` to return to when the theme picker (opened either
    /// directly or from inside the config editor) closes.
    pub theme_settings_return: View,

    /// Cursor into `theme::ALL` while the theme picker is open.
    pub settings_cursor: ListCursor,
    /// Theme active before the picker opened, restored on Esc-cancel.
    pub theme_preview_origin: Option<Theme>,

    /// Text currently typed into the `:` command palette.
    pub palette_input: String,
    /// Last palette result (message, is_error), shown until the next submit.
    pub palette_message: Option<PaletteMessage>,

    /// Working copy of `Config` edited by the config editor, persisted only
    /// on "Save & Close" — everything else in this view previews live but
    /// discards on Esc.
    pub config_draft: Config,
    /// What the config file looked like when [`Self::open_config_editor`] read
    /// it, so a save can tell whether the desktop app has written it since.
    /// `None` when the draft did not come from a file.
    pub config_draft_version: Option<ConfigVersion>,
    /// Cursor into `ConfigField::ALL`.
    pub config_editor_cursor: ListCursor,
    /// `Some(buffer)` while the selected text field is being edited.
    pub config_editor_editing: Option<String>,
    pub config_editor_error: Option<String>,

    /// Ticks forward while something on screen is animating, and stands still
    /// otherwise. Spinners and gradients read it; see [`TuiApp::is_animating`]
    /// for why it is not simply a wall clock.
    pub animation_frame: u64,

    /// `Some(frame_no)` while the startup splash animation is showing;
    /// checked first thing by `ui::draw`. Only ever set by the production
    /// runtime path — stays `None` in unit/render tests.
    pub splash_frame: Option<u64>,

    /// True while `<Space>` has been pressed and the which-key popup is
    /// waiting for the second key of the sequence, mirroring AstroNvim's
    /// leader behavior.
    pub leader_pending: bool,

    /// Which metric the history chart is plotting, as an index into the
    /// metric list `metrics()` returns. That list is always `Metrics::LEN`
    /// rows; the cursor still clamps on read as well as write because it does
    /// not own the list, which is what keeps "in range" the cursor's own
    /// property rather than every caller's job.
    pub history_cursor: ListCursor,

    /// Air-quality alerts waiting to be shown, worst first. The front one is
    /// what the toast is currently showing.
    air_alerts: alerts::AlertQueue,

    /// First key of a pending `g`-prefixed motion (vim's `gg`). Cleared as
    /// soon as the next key resolves it, so `g` followed by anything else is
    /// simply discarded rather than remembered.
    pub pending_g: bool,
}

impl TuiApp {
    pub fn new(configured_url: Option<DeviceUrl>, refresh_interval: RefreshInterval) -> Self {
        Self {
            current_snapshot: None,
            previous_successful_snapshot: None,
            reading_history: VecDeque::new(),
            last_fetch_duration: None,
            last_success_at: None,
            current_error: None,
            configured_url,
            refresh_interval,
            is_fetching: false,

            theme: Theme::default_theme(),
            config_path: None,
            history_path: None,
            view: View::Dashboard,
            theme_settings_return: View::Dashboard,

            settings_cursor: ListCursor::default(),
            theme_preview_origin: None,

            palette_input: String::new(),
            palette_message: None,

            config_draft: Config::default(),
            config_draft_version: None,
            config_editor_cursor: ListCursor::default(),
            config_editor_editing: None,
            config_editor_error: None,

            animation_frame: 0,
            splash_frame: None,
            history_cursor: ListCursor::default(),
            air_alerts: alerts::AlertQueue::new(),
            leader_pending: false,
            pending_g: false,
        }
    }

    /// Whether anything on screen needs a steady repaint.
    ///
    /// This gates every animation, because the alternative — repainting ten
    /// times a second forever — is the wrong trade for a tool left open all
    /// day on a laptop. A dashboard with nothing happening is a still picture
    /// and costs nothing to leave running.
    pub fn is_animating(&self) -> bool {
        self.is_fetching || self.current_alert().is_some()
    }

    /// Advances the animation clock by one frame. Called by the runtime while
    /// [`TuiApp::is_animating`] holds.
    pub fn advance_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
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
        // Before `current_snapshot` is replaced, so the comparison is against
        // the reading this one succeeds.
        self.raise_air_alerts(&snapshot, success_at);
        self.remember_successful_snapshot(Reading::new(success_at, snapshot.clone()));
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
        self.refresh_interval = self.refresh_interval.longer();
    }

    pub fn decrease_refresh_interval(&mut self) {
        self.refresh_interval = self.refresh_interval.shorter();
    }

    /// One metric's recent values, oldest first, for the trend sparkline.
    ///
    /// A reading the device did not report stays `None` so the graph can leave
    /// a gap rather than draw a drop to zero that never happened.
    pub fn metric_history(&self, key: &str) -> Vec<Option<f64>> {
        series(
            self.reading_history.iter().map(|reading| &reading.snapshot),
            key,
        )
    }

    pub fn trend_baseline(&self) -> Option<&SensorSnapshot> {
        self.previous_successful_snapshot.as_ref()
    }

    /// Keeps a reading for the chart, and records it to disk so the next
    /// session starts with a shape to draw instead of an empty axis.
    ///
    /// The write is best-effort, like every other persistence path in this
    /// module: a history file that cannot be written must not disturb the
    /// reading the user can already see on screen.
    fn remember_successful_snapshot(&mut self, reading: Reading) {
        if let Some(path) = &self.history_path {
            let _ = history::append(path, &reading);
        }

        self.reading_history.push_back(reading);
        while self.reading_history.len() > MAX_READING_HISTORY {
            self.reading_history.pop_front();
        }
    }

    /// Seeds the in-memory history from what previous sessions recorded.
    pub fn restore_history(&mut self, readings: impl IntoIterator<Item = Reading>) {
        self.reading_history = readings.into_iter().collect();
        let excess = self
            .reading_history
            .len()
            .saturating_sub(MAX_READING_HISTORY);
        self.reading_history.drain(..excess);
    }

    /// The recorded readings the chart draws, oldest first.
    pub fn reading_history(&self) -> impl ExactSizeIterator<Item = &Reading> {
        self.reading_history.iter()
    }

    /// The highlighted row in the theme picker.
    pub fn theme_cursor(&self) -> usize {
        self.settings_cursor.index(theme::ALL.len())
    }

    /// The highlighted row in the config editor.
    pub fn config_field_cursor(&self) -> usize {
        self.config_editor_cursor.index(ConfigField::ALL.len())
    }

    /// The metric rows to draw.
    ///
    /// Before the first successful fetch there is no snapshot, so an
    /// all-missing one stands in: the dashboard then has the same shape from
    /// frame one rather than popping into existence when data arrives. The UI
    /// layer used to do this substitution itself, on an empty `Vec` returned
    /// from here, which meant "no data yet" was represented twice.
    pub fn metrics(&self) -> Metrics {
        match &self.current_snapshot {
            Some(snapshot) => metrics(snapshot, self.trend_baseline()),
            None => metrics(&SensorSnapshot::default(), None),
        }
    }
}
