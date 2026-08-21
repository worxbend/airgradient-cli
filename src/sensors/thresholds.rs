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
    classify: fn(Option<f64>) -> Status,
    max: f64,
    /// The values at which this metric changes band, ascending. Empty for the
    /// two comfort scales, whose bands are ranges around a middle rather than
    /// a climb — a gauge draws those as one continuous run.
    boundaries: &'static [f64],
}

impl Scale {
    pub fn classify(self, value: Option<f64>) -> Status {
        (self.classify)(value)
    }

    /// The value at which this metric's gauge reads full. For most metrics
    /// that is where the worst status begins — past it, the gauge has nothing
    /// left to say that the colour is not already saying.
    pub const fn max(self) -> f64 {
        self.max
    }

    /// Where this metric changes band, for a gauge that marks them.
    pub const fn boundaries(self) -> &'static [f64] {
        self.boundaries
    }
}

/// The default exists only so `Metric` can derive `Deserialize` with the scale
/// skipped; a deserialized metric is a rendering input, not a classifier.
impl Default for Scale {
    fn default() -> Self {
        AQI
    }
}

impl PartialEq for Scale {
    fn eq(&self, other: &Self) -> bool {
        self.max == other.max && self.boundaries == other.boundaries
    }
}

impl fmt::Debug for Scale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scale")
            .field("max", &self.max)
            .field("boundaries", &self.boundaries)
            .finish()
    }
}

/// The values out of a breakpoint table, so a gauge marks exactly the points
/// the classifier switches at rather than a second list that could drift.
const fn boundaries_of(breakpoints: &[(f64, Status); 4]) -> [f64; 4] {
    [
        breakpoints[0].0,
        breakpoints[1].0,
        breakpoints[2].0,
        breakpoints[3].0,
    ]
}

const AQI_BOUNDARIES: [f64; 4] = boundaries_of(&AQI_BREAKPOINTS);
const CO2_BOUNDARIES: [f64; 4] = boundaries_of(&CO2_BREAKPOINTS);
const PM25_BOUNDARIES: [f64; 4] = boundaries_of(&PM25_BREAKPOINTS);
const PM10_BOUNDARIES: [f64; 4] = boundaries_of(&PM10_BREAKPOINTS);
const PARTICLE_COUNT_BOUNDARIES: [f64; 4] = boundaries_of(&PARTICLE_COUNT_BREAKPOINTS);
const TVOC_BOUNDARIES: [f64; 4] = boundaries_of(&TVOC_BREAKPOINTS);
const NOX_BOUNDARIES: [f64; 4] = boundaries_of(&NOX_BREAKPOINTS);

/// Temperature and humidity are comfort ranges, not a climb from good to bad:
/// both ends are uncomfortable and the middle is fine. A banded gauge would
/// have to run green-in-the-middle, so it is given no boundaries and drawn as
/// one continuous scale instead.
const COMFORT_BOUNDARIES: [f64; 0] = [];

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

/// The value at which the worst status begins, i.e. the last breakpoint.
const fn worst_status_begins_at(breakpoints: &[(f64, Status); 4]) -> f64 {
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

pub const AQI: Scale = Scale {
    classify: classify_aqi,
    max: AQI_SCALE_MAX,
    boundaries: &AQI_BOUNDARIES,
};
pub const CO2: Scale = Scale {
    classify: classify_co2,
    max: worst_status_begins_at(&CO2_BREAKPOINTS),
    boundaries: &CO2_BOUNDARIES,
};
pub const PM25: Scale = Scale {
    classify: classify_pm25,
    max: worst_status_begins_at(&PM25_BREAKPOINTS),
    boundaries: &PM25_BOUNDARIES,
};
pub const PM1: Scale = Scale {
    classify: classify_pm1,
    max: worst_status_begins_at(&PM25_BREAKPOINTS),
    boundaries: &PM25_BOUNDARIES,
};
pub const PM10: Scale = Scale {
    classify: classify_pm10,
    max: worst_status_begins_at(&PM10_BREAKPOINTS),
    boundaries: &PM10_BOUNDARIES,
};
pub const PARTICLE_COUNT: Scale = Scale {
    classify: classify_particles,
    max: worst_status_begins_at(&PARTICLE_COUNT_BREAKPOINTS),
    boundaries: &PARTICLE_COUNT_BOUNDARIES,
};
pub const TVOC: Scale = Scale {
    classify: classify_tvoc,
    max: worst_status_begins_at(&TVOC_BREAKPOINTS),
    boundaries: &TVOC_BOUNDARIES,
};
pub const NOX: Scale = Scale {
    classify: classify_nox,
    max: worst_status_begins_at(&NOX_BREAKPOINTS),
    boundaries: &NOX_BOUNDARIES,
};
pub const TEMPERATURE: Scale = Scale {
    classify: classify_temperature,
    max: TEMPERATURE_SCALE_MAX,
    boundaries: &COMFORT_BOUNDARIES,
};
pub const HUMIDITY: Scale = Scale {
    classify: classify_humidity,
    max: MAX_HUMIDITY_PERCENT,
    boundaries: &COMFORT_BOUNDARIES,
};

/// Relative humidity is a percentage; its gauge is the whole of it.
const MAX_HUMIDITY_PERCENT: f64 = 100.0;

fn classify_aqi(value: Option<f64>) -> Status {
    classify(value, &AQI_BREAKPOINTS)
}

fn classify_co2(value: Option<f64>) -> Status {
    classify(value, &CO2_BREAKPOINTS)
}

fn classify_pm25(value: Option<f64>) -> Status {
    classify(value, &PM25_BREAKPOINTS)
}

/// PM1.0 shares the PM2.5 breakpoints, deliberately.
///
/// Unlike PM2.5 and PM10, PM1.0 has no published EPA (or WHO) breakpoint table
/// to classify against — it is not a regulated size fraction. PM1.0 is a strict
/// subset of the particles PM2.5 measures, so a PM1.0 reading can never exceed
/// the PM2.5 reading from the same sensor, which makes the PM2.5 scale a
/// conservative stand-in: it can under-state a PM1.0 reading's severity but
/// never over-state it.
///
/// Kept as its own function rather than pointing the metric table at
/// [`classify_pm25`] so that the choice is visible where the numbers are, and
/// so a future published PM1.0 scale is a one-line change here.
fn classify_pm1(value: Option<f64>) -> Status {
    classify_pm25(value)
}

fn classify_pm10(value: Option<f64>) -> Status {
    classify(value, &PM10_BREAKPOINTS)
}

fn classify_particles(value: Option<f64>) -> Status {
    classify(value, &PARTICLE_COUNT_BREAKPOINTS)
}

fn classify_tvoc(value: Option<f64>) -> Status {
    classify(value, &TVOC_BREAKPOINTS)
}

fn classify_nox(value: Option<f64>) -> Status {
    classify(value, &NOX_BREAKPOINTS)
}

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
        assert_eq!(classify_aqi(None), Status::Unknown);
        assert_eq!(classify_aqi(Some(50.0)), Status::Good);
        assert_eq!(classify_aqi(Some(51.0)), Status::Moderate);
        assert_eq!(classify_aqi(Some(101.0)), Status::Elevated);
        assert_eq!(classify_aqi(Some(151.0)), Status::Unhealthy);
        assert_eq!(classify_aqi(Some(201.0)), Status::VeryUnhealthy);
    }

    #[test]
    fn classifies_humidity_and_temperature_boundaries() {
        assert_eq!(classify_temperature(Some(22.0)), Status::Good);
        assert_eq!(classify_temperature(Some(17.0)), Status::Moderate);
        assert_eq!(classify_temperature(Some(31.0)), Status::Elevated);
        assert_eq!(classify_humidity(Some(45.0)), Status::Good);
        assert_eq!(classify_humidity(Some(25.0)), Status::Moderate);
        assert_eq!(classify_humidity(Some(85.0)), Status::Unhealthy);
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
    fn every_scale_classifies_the_same_way_as_its_classifier() {
        assert_eq!(AQI.classify(Some(42.0)), classify_aqi(Some(42.0)));
        assert_eq!(PM1.classify(Some(42.0)), classify_pm1(Some(42.0)));
        assert_eq!(
            TEMPERATURE.classify(Some(22.0)),
            classify_temperature(Some(22.0))
        );
        assert_eq!(HUMIDITY.classify(None), Status::Unknown);
    }

    #[test]
    fn classifies_all_metric_families() {
        assert_eq!(classify_co2(Some(801.0)), Status::Moderate);
        assert_eq!(classify_pm25(Some(55.5)), Status::Unhealthy);
        assert_eq!(classify_pm10(Some(355.0)), Status::VeryUnhealthy);
        assert_eq!(classify_particles(Some(1001.0)), Status::Elevated);
        assert_eq!(classify_tvoc(Some(401.0)), Status::VeryUnhealthy);
        assert_eq!(classify_nox(Some(21.0)), Status::Elevated);
    }
}
