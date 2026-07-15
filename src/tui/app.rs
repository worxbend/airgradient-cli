use std::{
    collections::VecDeque,
    fmt,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use url::Url;

use crate::{
    config::{self, Config, MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS},
    device,
    sensors::{Metric, SensorSnapshot, metrics},
    tui::{
        command::{self, PaletteCommand},
        theme::{self, Theme},
    },
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

    // -- Theme settings (live-preview picker; Esc reverts, Enter persists) --

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

    fn persist_theme(&self, id: &str) {
        if let Some(path) = &self.config_path {
            let _ = config::set_theme(path, id);
        }
    }

    // -- Command palette (`:` verb commands: url/refresh/theme/config/save/quit) --

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
                let resolved = Theme::by_id(&id);
                self.theme = resolved;
                self.persist_theme(resolved.id);
                self.palette_message = Some((format!("theme set: {}", resolved.label), false));
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
                self.palette_message = Some(("config saved".to_string(), false));
                PaletteOutcome::Continue
            }
            Ok(PaletteCommand::Quit) => PaletteOutcome::Quit,
            Err(message) => {
                self.palette_message = Some((message, true));
                PaletteOutcome::Continue
            }
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

    // -- Config editor (multi-field form; per-field live preview, explicit
    // "Save & Close" persists everything at once; Esc discards the draft) --

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
        self.config_editor_cursor = self.config_editor_cursor.saturating_sub(1);
    }

    pub fn config_editor_nav_down(&mut self) {
        let max = ConfigField::ALL.len().saturating_sub(1);
        self.config_editor_cursor = (self.config_editor_cursor + 1).min(max);
    }

    pub fn config_editor_push_char(&mut self, c: char) {
        if let Some(buffer) = self.config_editor_editing.as_mut() {
            buffer.push(c);
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

fn clamp_refresh_interval(refresh_interval: Duration) -> Duration {
    refresh_interval.clamp(MIN_REFRESH_INTERVAL, MAX_REFRESH_INTERVAL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::Trend;

    fn app_with_refresh(refresh_interval: Duration) -> TuiApp {
        TuiApp::new(None, refresh_interval)
    }

    fn app_with_url() -> TuiApp {
        TuiApp::new(
            Some(Url::parse("http://192.168.1.201/").expect("url should parse")),
            Duration::from_secs(30),
        )
    }

    fn snapshot(aqi: f64, co2: f64) -> SensorSnapshot {
        SensorSnapshot {
            aqi: Some(aqi),
            co2: Some(co2),
            ..SensorSnapshot::default()
        }
    }

    #[test]
    fn success_records_current_snapshot_and_fetch_metadata() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        let success_at = SystemTime::UNIX_EPOCH + Duration::from_secs(42);
        let current = snapshot(42.0, 612.0);

        app.begin_fetch();
        app.finish_fetch_success(current.clone(), Duration::from_millis(128), success_at);

        assert_eq!(app.current_snapshot, Some(current));
        assert_eq!(app.previous_successful_snapshot, None);
        assert_eq!(app.successful_snapshots().count(), 1);
        assert_eq!(app.last_fetch_duration, Some(Duration::from_millis(128)));
        assert_eq!(app.last_success_at, Some(success_at));
        assert_eq!(app.current_error, None);
        assert!(!app.is_fetching);
    }

    #[test]
    fn failure_preserves_successful_snapshots_and_records_error() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        let first = snapshot(40.0, 600.0);
        let second = snapshot(45.0, 625.0);
        app.finish_fetch_success(
            first.clone(),
            Duration::from_millis(90),
            SystemTime::UNIX_EPOCH,
        );
        app.finish_fetch_success(
            second.clone(),
            Duration::from_millis(100),
            SystemTime::UNIX_EPOCH + Duration::from_secs(30),
        );

        app.begin_fetch();
        app.finish_fetch_failure("request timed out", Duration::from_millis(250));

        assert_eq!(app.current_snapshot, Some(second));
        assert_eq!(app.previous_successful_snapshot, Some(first));
        assert_eq!(app.last_fetch_duration, Some(Duration::from_millis(250)));
        assert_eq!(app.current_error.as_deref(), Some("request timed out"));
        assert!(!app.is_fetching);
    }

    #[test]
    fn success_history_keeps_recent_readings_for_terminal_traces() {
        let mut app = app_with_refresh(Duration::from_secs(30));

        for index in 0_u32..60 {
            app.finish_fetch_success(
                snapshot(f64::from(index), 600.0 + f64::from(index)),
                Duration::from_millis(50),
                SystemTime::UNIX_EPOCH + Duration::from_secs(index.into()),
            );
        }

        let history = app.successful_snapshots().collect::<Vec<_>>();
        assert_eq!(history.len(), MAX_READING_HISTORY);
        assert_eq!(
            history.first().and_then(|snapshot| snapshot.aqi),
            Some(12.0)
        );
        assert_eq!(history.last().and_then(|snapshot| snapshot.aqi), Some(59.0));
    }

    #[test]
    fn trend_baseline_is_available_after_two_successes() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        app.finish_fetch_success(
            snapshot(40.0, 650.0),
            Duration::from_millis(50),
            SystemTime::UNIX_EPOCH,
        );
        app.finish_fetch_success(
            snapshot(55.0, 620.0),
            Duration::from_millis(60),
            SystemTime::UNIX_EPOCH + Duration::from_secs(30),
        );

        assert_eq!(
            app.trend_baseline().and_then(|snapshot| snapshot.aqi),
            Some(40.0)
        );
        let metrics = app.metrics();
        assert_eq!(metrics[0].trend, Trend::Up);
        assert_eq!(metrics[1].trend, Trend::Down);
    }

    #[test]
    fn refresh_interval_is_clamped_and_adjusted_within_bounds() {
        let mut low = app_with_refresh(Duration::from_secs(1));
        assert_eq!(low.refresh_interval, MIN_REFRESH_INTERVAL);
        low.decrease_refresh_interval();
        assert_eq!(low.refresh_interval, MIN_REFRESH_INTERVAL);
        low.increase_refresh_interval();
        assert_eq!(low.refresh_interval, Duration::from_secs(10));

        let mut high = app_with_refresh(Duration::from_secs(4_000));
        assert_eq!(high.refresh_interval, MAX_REFRESH_INTERVAL);
        high.increase_refresh_interval();
        assert_eq!(high.refresh_interval, MAX_REFRESH_INTERVAL);
        high.decrease_refresh_interval();
        assert_eq!(
            high.refresh_interval,
            Duration::from_secs(MAX_REFRESH_INTERVAL_SECS - 5)
        );
    }

    #[test]
    fn begin_fetch_only_sets_pending_when_url_is_configured() {
        let mut missing_url = TuiApp::new(None, Duration::from_secs(30));
        missing_url.begin_fetch();
        assert!(!missing_url.is_fetching);

        let mut configured = app_with_url();
        configured.begin_fetch();
        assert!(configured.is_fetching);
    }

    #[test]
    fn finish_fetch_success_and_failure_clear_pending_state() {
        let mut success = app_with_url();
        success.begin_fetch();
        assert!(success.is_fetching);
        success.finish_fetch_success(
            snapshot(42.0, 612.0),
            Duration::from_millis(128),
            SystemTime::UNIX_EPOCH,
        );
        assert!(!success.is_fetching);

        let mut failure = app_with_url();
        failure.begin_fetch();
        assert!(failure.is_fetching);
        failure.finish_fetch_failure("request timed out", Duration::from_millis(250));
        assert!(!failure.is_fetching);
    }

    #[test]
    fn retry_preserves_previous_error_until_a_new_result_arrives() {
        let mut app = app_with_url();
        app.finish_fetch_failure("request timed out", Duration::from_millis(250));

        app.begin_fetch();

        assert!(app.is_fetching);
        assert_eq!(app.current_error.as_deref(), Some("request timed out"));

        app.finish_fetch_success(
            snapshot(42.0, 612.0),
            Duration::from_millis(128),
            SystemTime::UNIX_EPOCH,
        );

        assert!(!app.is_fetching);
        assert_eq!(app.current_error, None);
    }

    #[test]
    fn failure_after_retry_replaces_previous_error_and_retains_last_success() {
        let mut app = app_with_url();
        let successful = snapshot(42.0, 612.0);
        app.finish_fetch_success(
            successful.clone(),
            Duration::from_millis(128),
            SystemTime::UNIX_EPOCH,
        );
        app.finish_fetch_failure("request timed out", Duration::from_millis(250));

        app.begin_fetch();
        app.finish_fetch_failure("connection refused", Duration::from_millis(75));

        assert_eq!(app.current_snapshot, Some(successful));
        assert_eq!(app.current_error.as_deref(), Some("connection refused"));
        assert_eq!(app.last_fetch_duration, Some(Duration::from_millis(75)));
        assert!(!app.is_fetching);
    }

    fn app_with_config(dir: &std::path::Path) -> (TuiApp, std::path::PathBuf) {
        let path = dir.join("config.json");
        let mut app = app_with_refresh(Duration::from_secs(30));
        app.config_path = Some(path.clone());
        (app, path)
    }

    #[test]
    fn theme_settings_preview_reverts_on_close_without_confirm() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        let original = app.theme;

        app.open_theme_settings();
        app.theme_cursor_down();
        assert_ne!(app.theme.id, original.id);

        app.close_theme_settings();
        assert_eq!(app.theme.id, original.id);
        assert_eq!(app.view, View::Dashboard);
    }

    #[test]
    fn theme_settings_confirm_applies_and_persists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let (mut app, path) = app_with_config(temp_dir.path());

        app.open_theme_settings();
        app.theme_cursor_down();
        let chosen = app.theme;
        app.confirm_theme_settings();

        assert_eq!(app.theme.id, chosen.id);
        assert_eq!(app.view, View::Dashboard);
        assert_eq!(
            config::read_config(&path).unwrap().theme,
            chosen.id.to_string()
        );
    }

    #[test]
    fn palette_submit_sets_url_and_persists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let (mut app, path) = app_with_config(temp_dir.path());

        app.open_command_palette();
        for c in "url 192.168.1.201".chars() {
            app.palette_push_char(c);
        }
        let outcome = app.palette_submit();

        assert_eq!(outcome, PaletteOutcome::Continue);
        assert_eq!(
            app.configured_url.as_ref().map(|u| u.to_string()),
            Some("http://192.168.1.201/".to_string())
        );
        assert_eq!(
            config::read_config(&path).unwrap().server_url.as_deref(),
            Some("http://192.168.1.201/")
        );
    }

    #[test]
    fn palette_submit_rejects_out_of_range_refresh_interval() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        app.open_command_palette();
        for c in "refresh 1".chars() {
            app.palette_push_char(c);
        }

        app.palette_submit();

        assert_eq!(app.refresh_interval, Duration::from_secs(30));
        assert_eq!(
            app.palette_message.as_ref().map(|(_, is_error)| *is_error),
            Some(true)
        );
    }

    #[test]
    fn palette_submit_quit_returns_quit_outcome() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        app.open_command_palette();
        app.palette_push_char('q');

        assert_eq!(app.palette_submit(), PaletteOutcome::Quit);
    }

    #[test]
    fn palette_submit_unknown_command_shows_error_and_stays() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        app.open_command_palette();
        for c in "bogus".chars() {
            app.palette_push_char(c);
        }

        let outcome = app.palette_submit();

        assert_eq!(outcome, PaletteOutcome::Continue);
        assert_eq!(
            app.palette_message.as_ref().map(|(_, is_error)| *is_error),
            Some(true)
        );
    }

    #[test]
    fn config_editor_save_persists_draft_and_applies_live_state() {
        let temp_dir = tempfile::tempdir().unwrap();
        let (mut app, path) = app_with_config(temp_dir.path());

        app.open_config_editor();
        assert_eq!(app.config_editor_field(), ConfigField::ServerUrl);

        app.config_editor_confirm(); // begin editing server url
        for c in "192.168.1.50".chars() {
            app.config_editor_push_char(c);
        }
        app.config_editor_confirm(); // commit

        app.config_editor_nav_down();
        assert_eq!(app.config_editor_field(), ConfigField::RefreshInterval);
        app.config_editor_confirm(); // seeds the buffer with the current "30"
        app.config_editor_backspace();
        app.config_editor_backspace();
        for c in "45".chars() {
            app.config_editor_push_char(c);
        }
        app.config_editor_confirm();

        for _ in 0..(ConfigField::ALL.len() - 2) {
            app.config_editor_nav_down();
        }
        assert_eq!(app.config_editor_field(), ConfigField::SaveAndClose);
        app.config_editor_confirm();

        assert_eq!(app.view, View::Dashboard);
        assert_eq!(
            app.configured_url.as_ref().map(|u| u.to_string()),
            Some("http://192.168.1.50/".to_string())
        );
        assert_eq!(app.refresh_interval, Duration::from_secs(45));

        let persisted = config::read_config(&path).unwrap();
        assert_eq!(
            persisted.server_url.as_deref(),
            Some("http://192.168.1.50/")
        );
        assert_eq!(persisted.refresh_interval_secs, 45);
    }

    #[test]
    fn config_editor_esc_discards_draft() {
        let temp_dir = tempfile::tempdir().unwrap();
        let (mut app, path) = app_with_config(temp_dir.path());
        config::set_refresh_interval(&path, 90).unwrap();

        app.open_config_editor();
        app.config_editor_confirm(); // begin editing server url
        for c in "192.168.1.99".chars() {
            app.config_editor_push_char(c);
        }
        app.config_editor_cancel_edit();
        app.close_config_editor();

        assert_eq!(app.view, View::Dashboard);
        assert_eq!(app.configured_url, None);
        assert_eq!(
            config::read_config(&path).unwrap().refresh_interval_secs,
            90
        );
    }

    #[test]
    fn config_editor_toggles_booleans_immediately() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        app.open_config_editor();
        app.config_editor_nav_down();
        app.config_editor_nav_down();
        assert_eq!(app.config_editor_field(), ConfigField::NotificationsEnabled);

        let before = app.config_draft.notifications_enabled;
        app.config_editor_confirm();
        assert_eq!(app.config_draft.notifications_enabled, !before);
    }
}
