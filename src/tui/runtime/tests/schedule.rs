//! Refresh-interval policy: what the app model carries, and the rules the
//! diagnostic scheduler override has to obey.
//!
//! The bounds themselves are `RefreshInterval`'s and are tested there; what
//! this file checks is that the runtime never runs on an interval the app
//! model is not also reporting.

use super::*;

#[test]
fn the_app_model_carries_an_already_bounded_interval() {
    // `TuiApp` used to clamp on construction, which meant the bound was
    // enforced in two places under two different policies. It now takes a
    // `RefreshInterval`, so anything out of range was resolved before it got
    // here — deliberately, by whoever called `clamping`.
    assert_eq!(
        TuiApp::new(None, refresh(0)).refresh_interval,
        RefreshInterval::MIN
    );
    assert_eq!(
        TuiApp::new(None, refresh(4)).refresh_interval,
        RefreshInterval::MIN
    );
    assert_eq!(
        TuiApp::new(None, refresh(3601)).refresh_interval,
        RefreshInterval::MAX
    );
    assert_eq!(
        TuiApp::new(None, refresh(45)).refresh_interval.as_secs(),
        45
    );
}

fn assert_runtime_schedule_override(
    production_seconds: u64,
    override_value: Option<&str>,
    expected_schedule_interval: Duration,
) {
    let app = TuiApp::new(None, refresh(production_seconds));
    let production_interval = app.refresh_interval;

    assert_eq!(
        runtime_refresh_schedule_interval(production_interval.as_duration(), override_value),
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
