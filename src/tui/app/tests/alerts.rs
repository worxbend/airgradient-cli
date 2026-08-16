//! When the air is judged to have gone bad, and when it is judged to be back.

use super::*;
use crate::tui::app::AlertKind;
use std::time::UNIX_EPOCH;

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

/// A snapshot with one CO2 reading, which is enough to drive every band.
fn co2(ppm: f64) -> SensorSnapshot {
    SensorSnapshot {
        co2: Some(ppm),
        ..SensorSnapshot::default()
    }
}

/// Feeds readings in order and returns the app afterwards.
fn after_readings(values: [f64; 2]) -> TuiApp {
    let mut app = app_with_refresh(refresh(30));
    app.finish_fetch_success(co2(values[0]), Duration::from_millis(50), at(100));
    app.finish_fetch_success(co2(values[1]), Duration::from_millis(50), at(130));
    app
}

#[test]
fn the_first_reading_of_a_session_never_alerts() {
    // Otherwise every launch in a stuffy room opens with a wall of toasts for
    // conditions the user already knows about.
    let mut app = app_with_refresh(refresh(30));

    app.finish_fetch_success(co2(1800.0), Duration::from_millis(50), at(100));

    assert!(app.current_alert().is_none());
}

#[test]
fn crossing_into_a_concerning_band_raises_an_alert() {
    // 800 is Good, 1800 is Unhealthy.
    let app = after_readings([800.0, 1800.0]);

    let alert = app.current_alert().expect("crossing up should alert");
    assert_eq!(alert.metric_key, "co2");
    assert_eq!(alert.kind, AlertKind::Worsened);
    assert_eq!(alert.status, Status::Unhealthy);
    assert!(alert.message.contains("CO2"), "{}", alert.message);
    assert!(alert.message.contains("1800 ppm"), "{}", alert.message);
}

#[test]
fn staying_in_a_bad_band_does_not_alert_again() {
    // The whole point of edge-triggering: an alert every thirty seconds for a
    // condition that has not changed is one nobody reads.
    let mut app = after_readings([800.0, 1800.0]);
    assert!(app.current_alert().is_some());
    app.dismiss_alerts();

    app.finish_fetch_success(co2(1850.0), Duration::from_millis(50), at(160));

    assert!(app.current_alert().is_none());
}

#[test]
fn getting_worse_within_the_concerning_bands_alerts_again() {
    // Elevated to Unhealthy is news, even though both are already concerning.
    let mut app = after_readings([800.0, 1200.0]);
    app.dismiss_alerts();

    app.finish_fetch_success(co2(2500.0), Duration::from_millis(50), at(160));

    let alert = app.current_alert().expect("worsening further should alert");
    assert_eq!(alert.status, Status::VeryUnhealthy);
}

#[test]
fn improving_within_the_good_bands_does_not_alert() {
    let app = after_readings([900.0, 700.0]);

    assert!(app.current_alert().is_none());
}

#[test]
fn coming_back_out_of_a_concerning_band_announces_the_recovery() {
    let mut app = after_readings([800.0, 1800.0]);
    app.dismiss_alerts();

    app.finish_fetch_success(co2(700.0), Duration::from_millis(50), at(160));

    let alert = app.current_alert().expect("recovery should be announced");
    assert_eq!(alert.kind, AlertKind::Recovered);
    assert_eq!(alert.status, Status::Good);
    assert!(alert.message.contains("back to Good"), "{}", alert.message);
}

#[test]
fn a_sensor_dropping_out_or_returning_is_not_an_air_quality_change() {
    let mut app = app_with_refresh(refresh(30));
    app.finish_fetch_success(co2(1800.0), Duration::from_millis(50), at(100));
    app.dismiss_alerts();

    // The device stops reporting CO2 entirely.
    app.finish_fetch_success(
        SensorSnapshot::default(),
        Duration::from_millis(50),
        at(130),
    );
    assert!(
        app.current_alert().is_none(),
        "dropping out is not recovery"
    );

    // And starts again, still bad.
    app.finish_fetch_success(co2(1800.0), Duration::from_millis(50), at(160));
    assert!(
        app.current_alert().is_none(),
        "returning is not a worsening"
    );
}

#[test]
fn an_alert_expires_on_its_own_after_its_time_is_up() {
    let mut app = after_readings([800.0, 1800.0]);
    let raised_at = app.current_alert().expect("an alert").raised_at;

    // Still showing a moment later.
    assert!(!app.expire_due_alerts(raised_at + Duration::from_secs(1)));
    assert!(app.current_alert().is_some());

    // Gone once its time is up, and the loop is told to repaint.
    assert!(app.expire_due_alerts(raised_at + Duration::from_secs(30)));
    assert!(app.current_alert().is_none());
    // Nothing left to expire, so nothing to repaint for.
    assert!(!app.expire_due_alerts(raised_at + Duration::from_secs(60)));
}

#[test]
fn several_metrics_turning_at_once_queue_up_worst_first() {
    let mut app = app_with_refresh(refresh(30));
    app.finish_fetch_success(
        SensorSnapshot {
            co2: Some(700.0),
            pm25: Some(5.0),
            ..SensorSnapshot::default()
        },
        Duration::from_millis(50),
        at(100),
    );

    app.finish_fetch_success(
        SensorSnapshot {
            // Elevated.
            co2: Some(1200.0),
            // Very unhealthy — the worse news, so it leads.
            pm25: Some(200.0),
            ..SensorSnapshot::default()
        },
        Duration::from_millis(50),
        at(130),
    );

    let first = app.current_alert().expect("an alert").clone();
    assert_eq!(first.metric_key, "pm25");
    assert_eq!(first.status, Status::VeryUnhealthy);

    // The next one follows once the first has had its time.
    app.expire_due_alerts(first.raised_at + Duration::from_secs(30));
    let second = app.current_alert().expect("a second alert");
    assert_eq!(second.metric_key, "co2");
}
