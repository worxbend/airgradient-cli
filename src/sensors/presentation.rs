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

/// A metric's two glyphs: the Nerd Font one, and a plain-Unicode stand-in.
///
/// Nerd Font icons live in the Private Use Area, so a terminal without a
/// patched font draws them as an empty box. Every surface that shows an icon
/// also shows the metric's label, so the glyph never carries meaning on its
/// own — but a row of tofu still looks broken, which is what the fallback is
/// for. Which one is used is the theme's call: see `Theme::nerd_icons`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Icons {
    /// Nerd Font v3 glyph (Material Design Icons range).
    pub nerd: &'static str,
    /// Plain-Unicode stand-in, safe in any terminal.
    pub plain: &'static str,
}

impl Icons {
    pub const fn pick(self, nerd_fonts_available: bool) -> &'static str {
        if nerd_fonts_available {
            self.nerd
        } else {
            self.plain
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    Unknown,
    Stable,
    Up,
    Down,
}

impl Trend {
    const fn symbol(self) -> &'static str {
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
    /// The glyphs standing for this reading; pick one with [`Icons::pick`].
    pub icon: Icons,
    /// How this reading is judged and how far its gauge runs, carried here so
    /// a renderer never has to look a metric's scale up by its key.
    #[serde(skip)]
    pub scale: Scale,
    /// Decimal places this metric is written with. Carried for the same reason
    /// as `scale`: a renderer formatting a value that is *not* the current
    /// reading — an axis label, a bound — needs the metric's precision, and the
    /// only alternative was inferring it from `formatted_value`, which says
    /// nothing when the device is not reporting this metric.
    #[serde(skip)]
    pub decimals: usize,
}

impl Metric {
    /// This reading as text, or [`MISSING_VALUE`] when the device did not
    /// report it.
    pub fn display_value(&self) -> &str {
        self.formatted_value.as_deref().unwrap_or(MISSING_VALUE)
    }

    /// This reading with its unit — "612 ppm", "21.8 °C", or "--" — phrased
    /// once here so the alert banner, the cards and the text output cannot
    /// word the same reading three different ways.
    ///
    /// A metric with no unit (the AQI) is just its value, with no trailing
    /// space to trim.
    pub fn reading_text(&self) -> String {
        if self.unit.is_empty() {
            self.display_value().to_string()
        } else {
            format!("{} {}", self.display_value(), self.unit)
        }
    }

    /// Writes `value` with this metric's precision.
    ///
    /// For values that are not the current reading — axis labels, bounds —
    /// which is why it takes one rather than using `formatted_value`.
    pub fn format_reading(&self, value: f64) -> String {
        format_value(value, self.decimals)
    }
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

/// One metric's fixed definition: everything about it that does not depend on
/// a particular reading.
///
/// The `read` field is what makes this a table rather than ten hand-written
/// blocks — it is the only place that knows which [`SensorSnapshot`] field a
/// metric key refers to, so [`metrics`] and [`series`] cannot disagree about
/// it.
struct MetricSpec {
    key: &'static str,
    label: &'static str,
    unit: &'static str,
    /// Decimal places to format a reading to. Counts (CO2 ppm, particle
    /// counts, index values) are whole numbers; masses and temperatures are
    /// not.
    decimals: usize,
    /// Nerd Font glyph for this reading, and the plain-Unicode stand-in for
    /// terminals without a patched font. See [`Metric::icon`].
    icons: Icons,
    scale: Scale,
    read: fn(&SensorSnapshot) -> Option<f64>,
}

/// Every metric this tool reports, in the order every renderer shows them.
///
/// AQI is first, which [`Metrics`] relies on.
const SPECS: &[MetricSpec] = &[
    MetricSpec {
        key: "aqi",
        icons: Icons {
            nerd: "\u{f0599}",
            plain: "◒",
        },
        label: "AQI",
        unit: "",
        decimals: 0,
        scale: thresholds::AQI,
        read: |snapshot| snapshot.aqi,
    },
    MetricSpec {
        key: "co2",
        icons: Icons {
            nerd: "\u{f07e4}",
            plain: "•••",
        },
        label: "CO2",
        unit: "ppm",
        decimals: 0,
        scale: thresholds::CO2,
        read: |snapshot| snapshot.co2,
    },
    MetricSpec {
        key: "pm25",
        icons: Icons {
            nerd: "\u{f0d77}",
            plain: "✣",
        },
        label: "PM2.5",
        unit: "ug/m3",
        decimals: 1,
        scale: thresholds::PM25,
        read: |snapshot| snapshot.pm25,
    },
    MetricSpec {
        key: "pm1",
        icons: Icons {
            nerd: "\u{f0d77}",
            plain: "✣",
        },
        label: "PM1.0",
        unit: "ug/m3",
        decimals: 1,
        scale: thresholds::PM1,
        read: |snapshot| snapshot.pm1,
    },
    MetricSpec {
        key: "pm10",
        icons: Icons {
            nerd: "\u{f0d77}",
            plain: "✣",
        },
        label: "PM10",
        unit: "ug/m3",
        decimals: 1,
        scale: thresholds::PM10,
        read: |snapshot| snapshot.pm10,
    },
    MetricSpec {
        key: "pm03_count",
        icons: Icons {
            nerd: "\u{f0d77}",
            plain: "✣",
        },
        label: "PM0.3 count",
        unit: "count/dL",
        decimals: 0,
        scale: thresholds::PARTICLE_COUNT,
        read: |snapshot| snapshot.pm03_count,
    },
    MetricSpec {
        key: "tvoc",
        icons: Icons {
            nerd: "\u{f0093}",
            plain: "⚗",
        },
        label: "TVOC",
        unit: "index",
        decimals: 0,
        scale: thresholds::TVOC,
        read: |snapshot| snapshot.tvoc,
    },
    MetricSpec {
        key: "nox",
        icons: Icons {
            nerd: "\u{f0e9a}",
            plain: "≋",
        },
        label: "NOx",
        unit: "index",
        decimals: 0,
        scale: thresholds::NOX,
        read: |snapshot| snapshot.nox,
    },
    MetricSpec {
        key: "temperature_c",
        icons: Icons {
            nerd: "\u{f050f}",
            plain: "♨",
        },
        label: "Temperature",
        unit: "C",
        decimals: 1,
        scale: thresholds::TEMPERATURE,
        read: |snapshot| snapshot.temperature_c,
    },
    MetricSpec {
        key: "humidity",
        icons: Icons {
            nerd: "\u{f058e}",
            plain: "♢",
        },
        label: "Humidity",
        unit: "%",
        decimals: 1,
        scale: thresholds::HUMIDITY,
        read: |snapshot| snapshot.humidity,
    },
];

pub fn metrics(snapshot: &SensorSnapshot, previous: Option<&SensorSnapshot>) -> Metrics {
    let mut rows = SPECS
        .iter()
        .map(|spec| metric(spec, snapshot, previous))
        .collect::<Vec<_>>();
    // `SPECS` starts with AQI, so this cannot fail; draining from the front
    // rather than indexing keeps that the table's business and not this
    // function's.
    let aqi = rows.remove(0);

    Metrics { aqi, others: rows }
}

/// One metric's value across a run of readings, oldest first.
///
/// A reading the device did not report stays `None` rather than becoming a
/// zero, so a graph can leave a gap where a sensor dropped out instead of
/// drawing a plunge to the floor that never happened.
///
/// An unknown key yields an empty series rather than panicking: callers pass
/// keys from the same table, so this is unreachable, and a missing graph is a
/// better failure than a crashed dashboard.
pub fn series<'a>(
    snapshots: impl IntoIterator<Item = &'a SensorSnapshot>,
    key: &str,
) -> Vec<Option<f64>> {
    let Some(spec) = SPECS.iter().find(|spec| spec.key == key) else {
        return Vec::new();
    };

    snapshots
        .into_iter()
        .map(|snapshot| (spec.read)(snapshot))
        .collect()
}

fn metric(
    spec: &MetricSpec,
    snapshot: &SensorSnapshot,
    previous: Option<&SensorSnapshot>,
) -> Metric {
    let value = (spec.read)(snapshot);
    let previous = previous.and_then(spec.read);

    Metric {
        key: spec.key,
        label: spec.label,
        unit: spec.unit,
        value,
        formatted_value: value.map(|value| format_value(value, spec.decimals)),
        status: spec.scale.classify(value),
        trend: trend(value, previous),
        icon: spec.icons,
        scale: spec.scale,
        decimals: spec.decimals,
    }
}

fn format_value(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}")
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
