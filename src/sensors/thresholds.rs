//! Where a reading falls on the good-to-unhealthy scale.
//!
//! One [`Scale`] per measurement, each with the published range it comes from
//! recorded next to it. Kept apart from presentation so that revising a
//! threshold never means touching a renderer.
//!
//! A `Scale` bundles the classifier with the value its gauge fills at, because
//! the two are the same knowledge. The dashboard used to keep its own table of
//! gauge maxima, every entry of which was a copy of a number below; revising a
//! threshold left the fill bar disagreeing with the colour beside it.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Unknown,
    Good,
    Moderate,
    Elevated,
    Unhealthy,
    VeryUnhealthy,
}

impl Status {
    /// Where this sits on the good-to-unhealthy scale, for comparing two
    /// readings of the same metric.
    ///
    /// `Unknown` ranks below `Good` rather than alongside it: a sensor that
    /// stopped reporting is not a reading that improved, and nothing should
    /// alert on the transition either way.
    pub const fn rank(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Good => 1,
            Self::Moderate => 2,
            Self::Elevated => 3,
            Self::Unhealthy => 4,
            Self::VeryUnhealthy => 5,
        }
    }

    /// Whether a reading in this band is worth telling the user about
    /// unprompted. `Moderate` is deliberately below the line — it is the band
    /// an ordinary room sits in for much of the day, and an alert that fires
    /// every day is one nobody reads.
    pub const fn is_concerning(self) -> bool {
        self.rank() >= Self::Elevated.rank()
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Good => "Good",
            Self::Moderate => "Moderate",
            Self::Elevated => "Elevated",
            Self::Unhealthy => "Unhealthy",
            Self::VeryUnhealthy => "Very unhealthy",
        }
    }
}

/// How a reading is judged, and how far its gauge runs.
///
/// The two travel together so a renderer cannot pick up one without the other.
///
/// Equality and `Debug` compare the *scale*, not the function pointer inside
/// it: two scales with the same bounds and boundaries are the same scale as
/// far as anything here is concerned, and a raw pointer in a test failure's
/// output tells nobody anything.
#[derive(Clone, Copy)]
pub struct Scale {
    /// How a comfort scale is judged, for the two metrics whose bands are a
    /// range around a middle rather than a climb. `None` on every hazard
    /// scale, which is judged by its own `breakpoints`.
    comfort_classifier: Option<fn(Option<f64>) -> Status>,
    max: f64,
    /// The table this metric is judged by: the value each band *ends* at,
    /// ascending, paired with the status up to and including it. Empty for the
    /// two comfort scales, whose bands are ranges around a middle rather than a
    /// climb — a gauge draws those as one continuous run.
    ///
    /// This is the only copy of that table a hazard scale has: the classifier
    /// reads it and so does the gauge, so the points a gauge marks cannot
    /// drift from the points the classifier switches at.
    breakpoints: &'static [(f64, Status)],
}

impl Scale {
    /// A hazard scale: judged by its own breakpoints, with the gauge filling
    /// where the worst status begins.
    const fn banded(breakpoints: &'static [(f64, Status)]) -> Self {
        Self::banded_to(worst_status_begins_at(breakpoints), breakpoints)
    }

    /// A hazard scale whose gauge runs to `max` rather than to its last
    /// breakpoint. Only AQI needs this; see [`AQI_SCALE_MAX`].
    const fn banded_to(max: f64, breakpoints: &'static [(f64, Status)]) -> Self {
        Self {
            comfort_classifier: None,
            max,
            breakpoints,
        }
    }

    /// A comfort scale, judged by `judge` rather than by a table of bands.
    ///
    /// Pins `breakpoints` empty, so a comfort scale cannot carry a hazard
    /// table for a gauge to draw tick marks from.
    const fn comfort(max: f64, judge: fn(Option<f64>) -> Status) -> Self {
        Self {
            comfort_classifier: Some(judge),
            max,
            breakpoints: &COMFORT_BREAKPOINTS,
        }
    }

    pub fn classify(self, value: Option<f64>) -> Status {
        match self.comfort_classifier {
            Some(judge) => judge(value),
            None => classify(value, self.breakpoints),
        }
    }

    /// The value at which this metric's gauge reads full. For most metrics
    /// that is where the worst status begins — past it, the gauge has nothing
    /// left to say that the colour is not already saying.
    pub const fn max(self) -> f64 {
        self.max
    }

    /// Where this metric changes band, and what it becomes, for a gauge that
    /// marks them.
    pub const fn breakpoints(self) -> &'static [(f64, Status)] {
        self.breakpoints
    }
}

impl PartialEq for Scale {
    fn eq(&self, other: &Self) -> bool {
        self.max == other.max && self.breakpoints == other.breakpoints
    }
}

impl fmt::Debug for Scale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scale")
            .field("max", &self.max)
            .field("breakpoints", &self.breakpoints)
            .finish()
    }
}

/// Temperature and humidity are comfort ranges, not a climb from good to bad:
/// both ends are uncomfortable and the middle is fine. A banded gauge would
/// have to run green-in-the-middle, so it is given no boundaries and drawn as
/// one continuous scale instead.
const COMFORT_BREAKPOINTS: [(f64, Status); 0] = [];

/// The breakpoints each classifier compares against, named so the gauge maxima
/// below can be derived from them instead of restating them.
const AQI_BREAKPOINTS: [(f64, Status); 4] = [
    (50.0, Status::Good),
    (100.0, Status::Moderate),
    (150.0, Status::Elevated),
    (200.0, Status::Unhealthy),
];
const CO2_BREAKPOINTS: [(f64, Status); 4] = [
    (800.0, Status::Good),
    (1000.0, Status::Moderate),
    (1500.0, Status::Elevated),
    (2000.0, Status::Unhealthy),
];
const PM25_BREAKPOINTS: [(f64, Status); 4] = [
    (9.0, Status::Good),
    (35.4, Status::Moderate),
    (55.4, Status::Elevated),
    (125.4, Status::Unhealthy),
];
const PM10_BREAKPOINTS: [(f64, Status); 4] = [
    (54.0, Status::Good),
    (154.0, Status::Moderate),
    (254.0, Status::Elevated),
    (354.0, Status::Unhealthy),
];
const PARTICLE_COUNT_BREAKPOINTS: [(f64, Status); 4] = [
    (500.0, Status::Good),
    (1000.0, Status::Moderate),
    (2000.0, Status::Elevated),
    (5000.0, Status::Unhealthy),
];
const TVOC_BREAKPOINTS: [(f64, Status); 4] = [
    (100.0, Status::Good),
    (200.0, Status::Moderate),
    (300.0, Status::Elevated),
    (400.0, Status::Unhealthy),
];
const NOX_BREAKPOINTS: [(f64, Status); 4] = [
    (1.0, Status::Good),
    (20.0, Status::Moderate),
    (50.0, Status::Elevated),
    (100.0, Status::Unhealthy),
];

/// Where a banded gauge fills: the last breakpoint in the table.
///
/// Strictly, the worst status begins just *past* this value -- a reading equal
/// to the last breakpoint is still the second-worst band, because [`classify`]
/// matches on `value <= band_ends_at`. It is the right place for a gauge to
/// read full all the same: past it the bar has nothing left to say that the
/// colour is not already saying.
const fn worst_status_begins_at(breakpoints: &'static [(f64, Status)]) -> f64 {
    breakpoints[breakpoints.len() - 1].0
}

/// The AQI gauge is the exception: it runs the full 0-500 index rather than
/// stopping at the last breakpoint, because the index is *defined* up to 500
/// (see `air_quality::us_aqi_from_pm25`) and readings above 200 are exactly the
/// ones a user wants to see the remaining headroom for.
const AQI_SCALE_MAX: f64 = 500.0;

/// Comfort range rather than a hazard scale: a room thermometer that fills at
/// 40C is readable, one that fills at the sensor's -40..85 range is not.
const TEMPERATURE_SCALE_MAX: f64 = 40.0;

pub const AQI: Scale = Scale::banded_to(AQI_SCALE_MAX, &AQI_BREAKPOINTS);
pub const CO2: Scale = Scale::banded(&CO2_BREAKPOINTS);
pub const PM25: Scale = Scale::banded(&PM25_BREAKPOINTS);

/// PM1.0 shares the PM2.5 breakpoints, deliberately.
///
/// Unlike PM2.5 and PM10, PM1.0 has no published EPA (or WHO) breakpoint table
/// to classify against — it is not a regulated size fraction. PM1.0 is a strict
/// subset of the particles PM2.5 measures, so a PM1.0 reading can never exceed
/// the PM2.5 reading from the same sensor, which makes the PM2.5 scale a
/// conservative stand-in: it can under-state a PM1.0 reading's severity but
/// never over-state it.
///
/// A future published PM1.0 scale is a one-line change here.
pub const PM1: Scale = Scale::banded(&PM25_BREAKPOINTS);

pub const PM10: Scale = Scale::banded(&PM10_BREAKPOINTS);
pub const PARTICLE_COUNT: Scale = Scale::banded(&PARTICLE_COUNT_BREAKPOINTS);
pub const TVOC: Scale = Scale::banded(&TVOC_BREAKPOINTS);
pub const NOX: Scale = Scale::banded(&NOX_BREAKPOINTS);
pub const TEMPERATURE: Scale = Scale::comfort(TEMPERATURE_SCALE_MAX, classify_temperature);
pub const HUMIDITY: Scale = Scale::comfort(MAX_HUMIDITY_PERCENT, classify_humidity);

/// Relative humidity is a percentage; its gauge is the whole of it.
const MAX_HUMIDITY_PERCENT: f64 = 100.0;

fn classify_temperature(value: Option<f64>) -> Status {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return Status::Unknown;
    };

    if !(16.0..=30.0).contains(&value) {
        Status::Elevated
    } else if !(18.0..=26.0).contains(&value) {
        Status::Moderate
    } else {
        Status::Good
    }
}

fn classify_humidity(value: Option<f64>) -> Status {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return Status::Unknown;
    };

    if !(20.0..=80.0).contains(&value) {
        Status::Unhealthy
    } else if !(30.0..=60.0).contains(&value) {
        Status::Moderate
    } else {
        Status::Good
    }
}

fn classify(value: Option<f64>, breakpoints: &[(f64, Status)]) -> Status {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return Status::Unknown;
    };

    for (max, status) in breakpoints {
        if value <= *max {
            return *status;
        }
    }

    Status::VeryUnhealthy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concerning_starts_at_elevated_and_unknown_is_never_concerning() {
        assert!(!Status::Unknown.is_concerning());
        assert!(!Status::Good.is_concerning());
        assert!(!Status::Moderate.is_concerning());
        assert!(Status::Elevated.is_concerning());
        assert!(Status::Unhealthy.is_concerning());
        assert!(Status::VeryUnhealthy.is_concerning());
    }

    #[test]
    fn rank_orders_the_scale_with_unknown_at_the_bottom() {
        assert!(Status::Unknown.rank() < Status::Good.rank());
        assert!(Status::Good.rank() < Status::Moderate.rank());
        assert!(Status::Moderate.rank() < Status::Elevated.rank());
        assert!(Status::Elevated.rank() < Status::Unhealthy.rank());
        assert!(Status::Unhealthy.rank() < Status::VeryUnhealthy.rank());
    }

    #[test]
    fn classifies_aqi_boundaries() {
        assert_eq!(AQI.classify(None), Status::Unknown);
        assert_eq!(AQI.classify(Some(50.0)), Status::Good);
        assert_eq!(AQI.classify(Some(51.0)), Status::Moderate);
        assert_eq!(AQI.classify(Some(101.0)), Status::Elevated);
        assert_eq!(AQI.classify(Some(151.0)), Status::Unhealthy);
        assert_eq!(AQI.classify(Some(201.0)), Status::VeryUnhealthy);
    }

    #[test]
    fn classifies_humidity_and_temperature_boundaries() {
        assert_eq!(TEMPERATURE.classify(Some(22.0)), Status::Good);
        assert_eq!(TEMPERATURE.classify(Some(17.0)), Status::Moderate);
        assert_eq!(TEMPERATURE.classify(Some(31.0)), Status::Elevated);
        assert_eq!(HUMIDITY.classify(Some(45.0)), Status::Good);
        assert_eq!(HUMIDITY.classify(Some(25.0)), Status::Moderate);
        assert_eq!(HUMIDITY.classify(Some(85.0)), Status::Unhealthy);
    }

    #[test]
    fn gauge_maxima_track_the_breakpoints_they_are_derived_from() {
        // The dashboard reads these off the metric; nothing else may restate
        // them. A metric whose gauge fills before its worst status begins would
        // show a full bar next to an "Elevated" colour.
        for (scale, breakpoints) in [
            (CO2, CO2_BREAKPOINTS),
            (PM25, PM25_BREAKPOINTS),
            (PM1, PM25_BREAKPOINTS),
            (PM10, PM10_BREAKPOINTS),
            (PARTICLE_COUNT, PARTICLE_COUNT_BREAKPOINTS),
            (TVOC, TVOC_BREAKPOINTS),
            (NOX, NOX_BREAKPOINTS),
        ] {
            let worst_begins_at = breakpoints[breakpoints.len() - 1].0;
            assert_eq!(scale.max(), worst_begins_at);
            assert_eq!(scale.classify(Some(worst_begins_at)), Status::Unhealthy);
        }
    }

    #[test]
    fn aqi_and_comfort_gauges_use_their_full_range_not_a_breakpoint() {
        // Deliberate exceptions, called out so a future edit does not "fix"
        // them into the derived rule above.
        assert_eq!(AQI.max(), 500.0);
        assert_eq!(TEMPERATURE.max(), 40.0);
        assert_eq!(HUMIDITY.max(), 100.0);
    }

    #[test]
    fn every_banded_scale_changes_band_exactly_at_its_own_breakpoints() {
        // The gauge marks ticks at `breakpoints()` and colours cells by
        // `classify()`. This is the assertion that those two agree, for every
        // banded scale rather than a sampled few: a scale whose classifier
        // switched anywhere other than its own table would draw a colour
        // change away from the tick marking it.
        for scale in [
            AQI,
            CO2,
            PM25,
            PM1,
            PM10,
            PARTICLE_COUNT,
            TVOC,
            NOX,
        ] {
            for (band_ends_at, status) in scale.breakpoints() {
                assert_eq!(
                    scale.classify(Some(*band_ends_at)),
                    *status,
                    "the band ending at {band_ends_at} should still be {status:?}"
                );

                let just_past = band_ends_at + f64::EPSILON.max(band_ends_at.abs() * 1e-9);
                assert!(
                    scale.classify(Some(just_past)).rank() > status.rank(),
                    "just past {band_ends_at} should be worse than {status:?}"
                );
            }
        }

        // The comfort scales carry no table for a gauge to draw ticks from.
        assert!(TEMPERATURE.breakpoints().is_empty());
        assert!(HUMIDITY.breakpoints().is_empty());
        assert_eq!(HUMIDITY.classify(None), Status::Unknown);
    }

    #[test]
    fn classifies_all_metric_families() {
        assert_eq!(CO2.classify(Some(801.0)), Status::Moderate);
        assert_eq!(PM25.classify(Some(55.5)), Status::Unhealthy);
        assert_eq!(PM10.classify(Some(355.0)), Status::VeryUnhealthy);
        assert_eq!(PARTICLE_COUNT.classify(Some(1001.0)), Status::Elevated);
        assert_eq!(TVOC.classify(Some(401.0)), Status::VeryUnhealthy);
        assert_eq!(NOX.classify(Some(21.0)), Status::Elevated);
        assert_eq!(PM1.classify(Some(55.5)), Status::Unhealthy);
    }
}

