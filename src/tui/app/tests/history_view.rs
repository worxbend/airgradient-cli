//! The chart's state: which metric it plots, and the readings it plots from.

use super::*;
use crate::history::Reading;
use std::time::UNIX_EPOCH;

fn reading(at_secs: u64, aqi: f64, co2: f64) -> Reading {
    Reading::new(
        UNIX_EPOCH + Duration::from_secs(at_secs),
        SensorSnapshot {
            aqi: Some(aqi),
            co2: Some(co2),
            ..SensorSnapshot::default()
        },
    )
}

#[test]
fn history_opens_on_aqi_and_cycles_through_every_metric() {
    let mut app = app_with_refresh(refresh(30));
    app.open_history();

    assert_eq!(app.view, View::History);
    assert_eq!(app.history_metric().key, "aqi");

    app.history_next_metric();
    assert_eq!(app.history_metric().key, "co2");

    app.history_previous_metric();
    assert_eq!(app.history_metric().key, "aqi");

    // The ends clamp rather than wrapping, like every other list in the TUI.
    app.history_previous_metric();
    assert_eq!(app.history_metric().key, "aqi");

    app.history_last_metric();
    assert_eq!(app.history_metric().key, "humidity");
    app.history_next_metric();
    assert_eq!(app.history_metric().key, "humidity");

    app.history_first_metric();
    assert_eq!(app.history_metric().key, "aqi");
}

#[test]
fn the_plotted_series_follows_the_selected_metric() {
    let mut app = app_with_refresh(refresh(30));
    app.restore_history([reading(10, 40.0, 600.0), reading(20, 45.0, 650.0)]);

    assert_eq!(app.history_series(), vec![Some(40.0), Some(45.0)]);

    app.history_next_metric();
    assert_eq!(app.history_metric().key, "co2");
    assert_eq!(app.history_series(), vec![Some(600.0), Some(650.0)]);
}

#[test]
fn a_metric_the_device_never_reported_is_absent_rather_than_zero() {
    // Plotting a missing reading as 0 would draw a plunge to the floor that
    // never happened.
    let mut app = app_with_refresh(refresh(30));
    app.restore_history([reading(10, 40.0, 600.0)]);

    assert_eq!(app.metric_history("nox"), vec![None]);
}

#[test]
fn closing_the_chart_returns_to_the_dashboard() {
    let mut app = app_with_refresh(refresh(30));
    app.open_history();
    app.close_history();

    assert_eq!(app.view, View::Dashboard);
}

#[test]
fn restoring_history_keeps_only_the_most_recent_readings() {
    let mut app = app_with_refresh(refresh(30));
    let many: Vec<Reading> = (0..200).map(|i| reading(i, i as f64, 600.0)).collect();

    app.restore_history(many);

    let kept: Vec<&Reading> = app.reading_history().collect();
    assert!(kept.len() <= 48, "kept {}", kept.len());
    // The newest survived, not the oldest.
    assert_eq!(kept.last().unwrap().at_unix_secs, 199);
}

#[test]
fn successful_fetches_extend_the_history_with_their_own_timestamp() {
    let mut app = app_with_refresh(refresh(30));
    let at = UNIX_EPOCH + Duration::from_secs(1_000);

    app.finish_fetch_success(snapshot(42.0, 612.0), Duration::from_millis(80), at);

    let recorded: Vec<&Reading> = app.reading_history().collect();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].at(), at);
    assert_eq!(recorded[0].snapshot.aqi, Some(42.0));
}

#[test]
fn a_failed_fetch_records_nothing() {
    // A gap in the graph would otherwise mean "the network was down", which is
    // not what the graph is about.
    let mut app = app_with_refresh(refresh(30));
    app.finish_fetch_success(
        snapshot(42.0, 612.0),
        Duration::from_millis(80),
        UNIX_EPOCH + Duration::from_secs(1_000),
    );

    app.finish_fetch_failure("request timed out", Duration::from_millis(250));

    assert_eq!(app.reading_history().count(), 1);
}

#[test]
fn readings_are_recorded_to_disk_and_replay_into_the_next_session() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("history.jsonl");

    let mut session = app_with_refresh(refresh(30));
    session.history_path = Some(path.clone());
    session.finish_fetch_success(
        snapshot(42.0, 612.0),
        Duration::from_millis(80),
        UNIX_EPOCH + Duration::from_secs(1_000),
    );
    session.finish_fetch_success(
        snapshot(47.0, 640.0),
        Duration::from_millis(80),
        UNIX_EPOCH + Duration::from_secs(1_030),
    );
    drop(session);

    // What `runtime::run` does at startup.
    let mut next_session = app_with_refresh(refresh(30));
    next_session.restore_history(crate::history::load(&path).unwrap());

    assert_eq!(
        next_session.metric_history("aqi"),
        vec![Some(42.0), Some(47.0)]
    );
}

#[test]
fn a_session_without_a_history_path_never_touches_the_filesystem() {
    let mut app = app_with_refresh(refresh(30));
    assert_eq!(app.history_path, None);

    app.finish_fetch_success(
        snapshot(42.0, 612.0),
        Duration::from_millis(80),
        UNIX_EPOCH + Duration::from_secs(1_000),
    );

    // Still kept in memory for the current session's chart.
    assert_eq!(app.reading_history().count(), 1);
}
