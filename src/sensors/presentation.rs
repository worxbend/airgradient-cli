use serde::{Deserialize, Serialize};

use super::air_quality::SensorSnapshot;
use super::thresholds::{
    Status, classify_aqi, classify_co2, classify_humidity, classify_nox, classify_particles,
    classify_pm10, classify_pm25, classify_temperature, classify_tvoc,
};

/// What every renderer shows in place of a reading the device did not report.
///
/// Defined once here, next to the metrics it applies to, so the text, JSON,
/// and TUI outputs cannot drift into using different placeholders for the
/// same absent value.
pub const MISSING_VALUE: &str = "--";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    Unknown,
    Stable,
    Up,
    Down,
}

impl Trend {
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Unknown => "",
            Self::Stable => "-",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    /// The trend as shown to a user. An unknown trend has no symbol of its
    /// own — there is nothing to compare against yet — so it reads as the
    /// missing-value placeholder rather than as an empty cell.
    pub const fn display_symbol(self) -> &'static str {
        match self {
            Self::Unknown => MISSING_VALUE,
            other => other.symbol(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub value: Option<f64>,
    pub formatted_value: Option<String>,
    pub status: Status,
    pub trend: Trend,
}

pub fn metrics(snapshot: &SensorSnapshot, previous: Option<&SensorSnapshot>) -> Vec<Metric> {
    vec![
        metric(
            "aqi",
            "AQI",
            "",
            snapshot.aqi,
            previous.and_then(|snapshot| snapshot.aqi),
            0,
            classify_aqi,
        ),
        metric(
            "co2",
            "CO2",
            "ppm",
            snapshot.co2,
            previous.and_then(|snapshot| snapshot.co2),
            0,
            classify_co2,
        ),
        metric(
            "pm25",
            "PM2.5",
            "ug/m3",
            snapshot.pm25,
            previous.and_then(|snapshot| snapshot.pm25),
            1,
            classify_pm25,
        ),
        metric(
            "pm1",
            "PM1.0",
            "ug/m3",
            snapshot.pm1,
            previous.and_then(|snapshot| snapshot.pm1),
            1,
            classify_pm25,
        ),
        metric(
            "pm10",
            "PM10",
            "ug/m3",
            snapshot.pm10,
            previous.and_then(|snapshot| snapshot.pm10),
            1,
            classify_pm10,
        ),
        metric(
            "pm03_count",
            "PM0.3 count",
            "count/dL",
            snapshot.pm03_count,
            previous.and_then(|snapshot| snapshot.pm03_count),
            0,
            classify_particles,
        ),
        metric(
            "tvoc",
            "TVOC",
            "index",
            snapshot.tvoc,
            previous.and_then(|snapshot| snapshot.tvoc),
            0,
            classify_tvoc,
        ),
        metric(
            "nox",
            "NOx",
            "index",
            snapshot.nox,
            previous.and_then(|snapshot| snapshot.nox),
            0,
            classify_nox,
        ),
        metric(
            "temperature_c",
            "Temperature",
            "C",
            snapshot.temperature_c,
            previous.and_then(|snapshot| snapshot.temperature_c),
            1,
            classify_temperature,
        ),
        metric(
            "humidity",
            "Humidity",
            "%",
            snapshot.humidity,
            previous.and_then(|snapshot| snapshot.humidity),
            1,
            classify_humidity,
        ),
    ]
}

fn metric(
    key: &'static str,
    label: &'static str,
    unit: &'static str,
    value: Option<f64>,
    previous: Option<f64>,
    decimals: usize,
    classify: fn(Option<f64>) -> Status,
) -> Metric {
    Metric {
        key,
        label,
        unit,
        value,
        formatted_value: value.map(|value| format_value(value, decimals)),
        status: classify(value),
        trend: trend(value, previous),
    }
}

fn format_value(value: f64, decimals: usize) -> String {
    if decimals == 0 {
        format!("{value:.0}")
    } else {
        format!("{value:.decimals$}")
    }
}

fn trend(value: Option<f64>, previous: Option<f64>) -> Trend {
    let (Some(value), Some(previous)) = (value, previous) else {
        return Trend::Unknown;
    };

    let delta = value - previous;
    if delta.abs() < 0.05 {
        Trend::Stable
    } else if delta > 0.0 {
        Trend::Up
    } else {
        Trend::Down
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_trend_displays_as_the_missing_value_placeholder() {
        // The text, JSON, and TUI renderers all go through this, so a drift
        // here would show three different placeholders for the same state.
        assert_eq!(Trend::Unknown.symbol(), "");
        assert_eq!(Trend::Unknown.display_symbol(), MISSING_VALUE);
    }

    #[test]
    fn known_trends_display_as_their_own_symbol() {
        for trend in [Trend::Stable, Trend::Up, Trend::Down] {
            assert_eq!(trend.display_symbol(), trend.symbol());
            assert_ne!(trend.display_symbol(), MISSING_VALUE);
        }
    }

    #[test]
    fn builds_expected_metric_definitions() {
        let snapshot = SensorSnapshot {
            aqi: Some(42.0),
            co2: Some(612.0),
            pm25: Some(4.2),
            ..SensorSnapshot::default()
        };

        let metrics = metrics(&snapshot, None);

        assert_eq!(metrics.len(), 10);
        assert_eq!(metrics[0].label, "AQI");
        assert_eq!(metrics[0].formatted_value.as_deref(), Some("42"));
        assert_eq!(metrics[2].label, "PM2.5");
        assert_eq!(metrics[2].unit, "ug/m3");
        assert_eq!(metrics[3].formatted_value, None);
    }

    #[test]
    fn calculates_trends_against_previous_snapshot() {
        let current = SensorSnapshot {
            aqi: Some(50.0),
            co2: Some(750.0),
            pm25: Some(10.0),
            humidity: None,
            ..SensorSnapshot::default()
        };
        let previous = SensorSnapshot {
            aqi: Some(50.0),
            co2: Some(800.0),
            pm25: Some(9.0),
            humidity: Some(45.0),
            ..SensorSnapshot::default()
        };

        let metrics = metrics(&current, Some(&previous));

        assert_eq!(metrics[0].trend, Trend::Stable);
        assert_eq!(metrics[1].trend, Trend::Down);
        assert_eq!(metrics[2].trend, Trend::Up);
        assert_eq!(metrics[9].trend, Trend::Unknown);
    }
}
