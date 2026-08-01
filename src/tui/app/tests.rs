//! Tests for the app model: fetch lifecycle and history, refresh-interval
//! bounds, and the palette / theme-picker / config-editor state machines.

use super::*;
use crate::{config, sensors::Trend};

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
