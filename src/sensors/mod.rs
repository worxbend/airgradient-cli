//! Sensor readings: parsing them, judging them, and presenting them.
//!
//! [`air_quality`] turns a device payload into a snapshot, [`thresholds`]
//! classifies a reading as good/elevated/unhealthy, and [`presentation`]
//! combines the two into the metric rows every renderer draws.

pub mod air_quality;
pub mod presentation;
pub mod thresholds;

pub use air_quality::{SensorSnapshot, parse_snapshot, us_aqi_from_pm25};
pub use presentation::{MISSING_VALUE, Metric, Trend, metrics};
pub use thresholds::Status;
