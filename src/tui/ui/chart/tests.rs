//! Tests for the reading chart.
//!
//! A child module rather than an inline block, so `mod.rs` stays within the
//! file-size limit while the tests keep reaching the private items they
//! exercise (`PlotBounds`, `split_into_bands`) through `use super::*`.

use super::*;

#[test]
fn bounds_pad_the_range_without_going_below_zero() {
    let bounds = PlotBounds::of(&[(-60.0, 10.0), (-30.0, 20.0), (0.0, 15.0)])
        .expect("points should yield bounds");

    assert_eq!(bounds.x_min, -60.0);
    assert_eq!(bounds.x_max, 0.0);
    // Padded above, floored at zero below rather than going negative.
    assert!(bounds.y_max > 20.0);
    assert!(bounds.y_min >= 0.0);
    assert!(bounds.y_min < 10.0);
}

#[test]
fn a_flat_series_still_gets_a_range_to_draw_in() {
    let bounds = PlotBounds::of(&[(-60.0, 42.0), (0.0, 42.0)]).expect("points should yield bounds");

    assert!(
        bounds.y_max > bounds.y_min,
        "a zero-height axis would put every point in one cell"
    );
    assert!(bounds.y_min < 42.0 && bounds.y_max > 42.0);
}

#[test]
fn a_single_instant_still_has_a_non_zero_width() {
    let bounds = PlotBounds::of(&[(0.0, 5.0)]).expect("one point should yield bounds");

    assert!(bounds.x_max > bounds.x_min);
}

fn co2(ppm: f64) -> Metric {
    let snapshot = crate::sensors::SensorSnapshot {
        co2: Some(ppm),
        ..crate::sensors::SensorSnapshot::default()
    };
    crate::sensors::metrics(&snapshot, None)
        .by_key("co2")
        .expect("co2 is a metric")
        .clone()
}

#[test]
fn a_line_that_stays_in_one_band_is_one_segment() {
    let metric = co2(700.0);
    let bands = split_into_bands(&[(-60.0, 600.0), (-30.0, 700.0), (0.0, 750.0)], &metric);

    assert_eq!(bands.len(), 1);
    assert_eq!(bands[0].0, Status::Good);
    assert_eq!(bands[0].1.len(), 3);
}

#[test]
fn crossing_a_band_splits_the_line_and_the_halves_still_touch() {
    // 700 is Good, 1200 is Elevated.
    let metric = co2(1200.0);
    let bands = split_into_bands(&[(-60.0, 700.0), (0.0, 1200.0)], &metric);

    assert_eq!(bands.len(), 2);
    assert_eq!(bands[0].0, Status::Good);
    assert_eq!(bands[1].0, Status::Elevated);
    // The second segment starts where the first ended, so the coloured
    // lines join instead of leaving a gap.
    assert_eq!(bands[0].1.last(), bands[1].1.first());
}

#[test]
fn no_points_means_no_bands_rather_than_an_empty_segment() {
    assert!(split_into_bands(&[], &co2(700.0)).is_empty());
}

#[test]
fn zero_readings_have_no_bounds() {
    assert!(PlotBounds::of(&[]).is_none());
}

#[test]
fn axis_values_follow_the_metrics_own_precision() {
    let whole = Metric {
        key: "co2",
        label: "CO2",
        unit: "ppm",
        value: Some(612.0),
        formatted_value: Some("612".to_string()),
        status: crate::sensors::Status::Good,
        trend: crate::sensors::Trend::Stable,
        scale: crate::sensors::thresholds::CO2,
        icon: crate::sensors::Icons {
            nerd: "",
            plain: "",
        },
        decimals: 0,
    };
    let fractional = Metric {
        decimals: 1,
        formatted_value: Some("7.4".to_string()),
        ..whole.clone()
    };

    assert_eq!(whole.format_reading(612.4), "612");
    assert_eq!(fractional.format_reading(7.44), "7.4");
}

/// A PM2.5 metric, optionally reporting a value.
fn pm25_metric(value: Option<f64>) -> Metric {
    let snapshot = crate::sensors::SensorSnapshot {
        pm25: value,
        ..crate::sensors::SensorSnapshot::default()
    };
    crate::sensors::metrics(&snapshot, None)
        .by_key("pm25")
        .expect("pm25 is a metric")
        .clone()
}

#[test]
fn axis_precision_survives_a_metric_the_device_is_not_reporting() {
    // The precision used to be inferred by looking for a "." inside the
    // *current* formatted reading. A metric the device is not reporting has
    // no formatted reading, so a PM2.5 chart drawn from recorded history
    // lost its decimal and drew 0/5/10 for a range where the decimal is the
    // whole point.
    let reported = pm25_metric(Some(7.4));
    let absent = pm25_metric(None);

    assert_eq!(reported.format_reading(7.44), "7.4");
    assert_eq!(
        absent.format_reading(7.44),
        "7.4",
        "precision belongs to the metric, not to whether it has a reading"
    );
}
