//! Refresh-interval policy: the app-model clamp, and the rules the
//! diagnostic scheduler override has to obey.

use super::*;

#[test]
fn production_refresh_clamp_keeps_documented_bounds() {
    assert_eq!(
        TuiApp::new(None, Duration::from_secs(0)).refresh_interval,
        Duration::from_secs(MIN_REFRESH_INTERVAL_SECS)
    );
    assert_eq!(
        TuiApp::new(None, Duration::from_secs(1)).refresh_interval,
        Duration::from_secs(MIN_REFRESH_INTERVAL_SECS)
    );
    assert_eq!(
        TuiApp::new(None, Duration::from_secs(4)).refresh_interval,
        Duration::from_secs(MIN_REFRESH_INTERVAL_SECS)
    );
    assert_eq!(
        TuiApp::new(None, Duration::from_secs(5)).refresh_interval,
        Duration::from_secs(MIN_REFRESH_INTERVAL_SECS)
    );
    assert_eq!(
        TuiApp::new(None, Duration::from_secs(3601)).refresh_interval,
        Duration::from_secs(MAX_REFRESH_INTERVAL_SECS)
    );
}

fn assert_runtime_schedule_override(
    production_seconds: u64,
    override_value: Option<&str>,
    expected_schedule_interval: Duration,
) {
    let app = TuiApp::new(None, Duration::from_secs(production_seconds));
    let production_interval = app.refresh_interval;

    assert_eq!(
        runtime_refresh_schedule_interval(production_interval, override_value),
        expected_schedule_interval
    );
    assert_eq!(app.refresh_interval, production_interval);
}

#[test]
fn runtime_refresh_schedule_override_can_shorten_without_changing_app_interval() {
    assert_runtime_schedule_override(
        MIN_REFRESH_INTERVAL_SECS,
        Some("250"),
        Duration::from_millis(250),
    );
}

#[test]
fn minimum_runtime_refresh_schedule_override_can_shorten_without_changing_app_interval() {
    assert_runtime_schedule_override(
        MIN_REFRESH_INTERVAL_SECS,
        Some("100"),
        Duration::from_millis(100),
    );
}

#[test]
fn invalid_runtime_refresh_schedule_override_keeps_production_interval() {
    let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

    assert_runtime_schedule_override(
        MIN_REFRESH_INTERVAL_SECS,
        Some("invalid"),
        production_interval,
    );
}

#[test]
fn below_minimum_runtime_refresh_schedule_override_keeps_production_interval() {
    let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

    assert_runtime_schedule_override(MIN_REFRESH_INTERVAL_SECS, Some("99"), production_interval);
}

#[test]
fn zero_runtime_refresh_schedule_override_keeps_production_interval() {
    let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

    assert_runtime_schedule_override(MIN_REFRESH_INTERVAL_SECS, Some("0"), production_interval);
}

#[test]
fn equal_runtime_refresh_schedule_override_keeps_production_interval() {
    let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

    assert_runtime_schedule_override(MIN_REFRESH_INTERVAL_SECS, Some("5000"), production_interval);
}

#[test]
fn longer_runtime_refresh_schedule_override_keeps_production_interval() {
    let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

    assert_runtime_schedule_override(MIN_REFRESH_INTERVAL_SECS, Some("6000"), production_interval);
}

#[test]
fn missing_runtime_refresh_schedule_override_keeps_production_interval() {
    let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);

    assert_runtime_schedule_override(MIN_REFRESH_INTERVAL_SECS, None, production_interval);
}
