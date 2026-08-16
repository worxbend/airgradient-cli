//! Fetch lifecycle: what a success or a failure does to the current and
//! previous snapshots, the pending flag, and the recorded error — plus the
//! refresh-interval bounds.

use super::*;

#[test]
fn success_records_current_snapshot_and_fetch_metadata() {
    let mut app = app_with_refresh(refresh(30));
    let success_at = SystemTime::UNIX_EPOCH + Duration::from_secs(42);
    let current = snapshot(42.0, 612.0);

    app.begin_fetch();
    app.finish_fetch_success(current.clone(), Duration::from_millis(128), success_at);

    assert_eq!(app.current_snapshot, Some(current));
    assert_eq!(app.previous_successful_snapshot, None);
    assert_eq!(app.last_fetch_duration, Some(Duration::from_millis(128)));
    assert_eq!(app.last_success_at, Some(success_at));
    assert_eq!(app.current_error, None);
    assert!(!app.is_fetching);
}

#[test]
fn failure_preserves_successful_snapshots_and_records_error() {
    let mut app = app_with_refresh(refresh(30));
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
fn trend_baseline_is_available_after_two_successes() {
    let mut app = app_with_refresh(refresh(30));
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
    let mut low = app_with_refresh(refresh(1));
    assert_eq!(low.refresh_interval, RefreshInterval::MIN);
    low.decrease_refresh_interval();
    assert_eq!(low.refresh_interval, RefreshInterval::MIN);
    low.increase_refresh_interval();
    assert_eq!(low.refresh_interval, refresh(10));

    let mut high = app_with_refresh(refresh(4_000));
    assert_eq!(high.refresh_interval, RefreshInterval::MAX);
    high.increase_refresh_interval();
    assert_eq!(high.refresh_interval, RefreshInterval::MAX);
    high.decrease_refresh_interval();
    assert_eq!(
        high.refresh_interval,
        refresh(MAX_REFRESH_INTERVAL_SECS - 5)
    );
}

#[test]
fn begin_fetch_only_sets_pending_when_url_is_configured() {
    let mut missing_url = TuiApp::new(None, refresh(30));
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
