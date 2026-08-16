//! The metric rows every renderer draws.
//!
//! Owns the order the metrics appear in, their labels and units, the
//! placeholder for a missing reading, and the trend against the previous
//! snapshot — so the text, JSON, and TUI outputs cannot disagree about any of
//! it. It does not decide what a value *means*; that is [`super::thresholds`].

use serde::{Deserialize, Serialize};

use super::air_quality::SensorSnapshot;
use super::thresholds::{self, Scale, Status};

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
    /// The value this metric's gauge reads full at, carried here so a renderer
    /// never has to look a metric's scale up by its key.
    #[serde(skip)]
    pub scale_max: f64,
}

/// The metric rows for one reading, with the AQI row reachable by name.
///
/// Every renderer leads with AQI and so had to go looking for it, each with its
/// own `find(|m| m.key == "aqi").expect(...)`. The expectation was sound — the
/// list below always starts with one — but it was asserted at the call sites
/// rather than guaranteed where the list is built. Holding the AQI row
/// separately makes it a fact of the type: [`Metrics::aqi`] cannot fail, so
/// there is nothing left for a caller to assert or to get wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct Metrics {
    aqi: Metric,
    others: Vec<Metric>,
}

impl Metrics {
    /// The AQI row, which always exists.
    pub fn aqi(&self) -> &Metric {
        &self.aqi
    }

    /// Every row, in display order — AQI first.
    pub fn iter(&self) -> impl Iterator<Item = &Metric> {
        std::iter::once(&self.aqi).chain(self.others.iter())
    }

    pub fn by_key(&self, key: &str) -> Option<&Metric> {
        self.iter().find(|metric| metric.key == key)
    }

    pub fn len(&self) -> usize {
        self.others.len() + 1
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

impl<'a> IntoIterator for &'a Metrics {
    type Item = &'a Metric;
    type IntoIter = Box<dyn Iterator<Item = &'a Metric> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

pub fn metrics(snapshot: &SensorSnapshot, previous: Option<&SensorSnapshot>) -> Metrics {
    let aqi = metric(
        "aqi",
        "AQI",
        "",
        snapshot.aqi,
        previous.and_then(|snapshot| snapshot.aqi),
        0,
        thresholds::AQI,
    );

    let others = vec![
        metric(
            "co2",
            "CO2",
            "ppm",
            snapshot.co2,
            previous.and_then(|snapshot| snapshot.co2),
            0,
            thresholds::CO2,
        ),
        metric(
            "pm25",
            "PM2.5",
            "ug/m3",
            snapshot.pm25,
            previous.and_then(|snapshot| snapshot.pm25),
            1,
            thresholds::PM25,
        ),
        metric(
            "pm1",
            "PM1.0",
            "ug/m3",
            snapshot.pm1,
            previous.and_then(|snapshot| snapshot.pm1),
            1,
            thresholds::PM1,
        ),
        metric(
            "pm10",
            "PM10",
            "ug/m3",
            snapshot.pm10,
            previous.and_then(|snapshot| snapshot.pm10),
            1,
            thresholds::PM10,
        ),
        metric(
            "pm03_count",
            "PM0.3 count",
            "count/dL",
            snapshot.pm03_count,
            previous.and_then(|snapshot| snapshot.pm03_count),
            0,
            thresholds::PARTICLE_COUNT,
        ),
        metric(
            "tvoc",
            "TVOC",
            "index",
            snapshot.tvoc,
            previous.and_then(|snapshot| snapshot.tvoc),
            0,
            thresholds::TVOC,
        ),
        metric(
            "nox",
            "NOx",
            "index",
            snapshot.nox,
            previous.and_then(|snapshot| snapshot.nox),
            0,
            thresholds::NOX,
        ),
        metric(
            "temperature_c",
            "Temperature",
            "C",
            snapshot.temperature_c,
            previous.and_then(|snapshot| snapshot.temperature_c),
            1,
            thresholds::TEMPERATURE,
        ),
        metric(
            "humidity",
            "Humidity",
            "%",
            snapshot.humidity,
            previous.and_then(|snapshot| snapshot.humidity),
            1,
            thresholds::HUMIDITY,
        ),
    ];

    Metrics { aqi, others }
}

fn metric(
    key: &'static str,
    label: &'static str,
    unit: &'static str,
    value: Option<f64>,
    previous: Option<f64>,
    decimals: usize,
    scale: Scale,
) -> Metric {
    Metric {
        key,
        label,
        unit,
        value,
        formatted_value: value.map(|value| format_value(value, decimals)),
        status: scale.classify(value),
        trend: trend(value, previous),
        scale_max: scale.max(),
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
        assert_eq!(metrics.aqi().label, "AQI");
        assert_eq!(metrics.aqi().formatted_value.as_deref(), Some("42"));

        let pm25 = metrics.by_key("pm25").expect("pm25 is one of the rows");
        assert_eq!(pm25.label, "PM2.5");
        assert_eq!(pm25.unit, "ug/m3");
        assert_eq!(
            metrics
                .by_key("pm1")
                .and_then(|m| m.formatted_value.clone()),
            None
        );

        // AQI leads the list, which is what `aqi()` promises callers.
        assert_eq!(metrics.iter().next().map(|m| m.key), Some("aqi"));
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

        let trend_of = |key: &str| metrics.by_key(key).map(|metric| metric.trend);
        assert_eq!(metrics.aqi().trend, Trend::Stable);
        assert_eq!(trend_of("co2"), Some(Trend::Down));
        assert_eq!(trend_of("pm25"), Some(Trend::Up));
        assert_eq!(trend_of("humidity"), Some(Trend::Unknown));
    }
}
