//! How often measurements are re-fetched.
//!
//! Like [`crate::device_url`], this is a rule with no I/O attached, depended on
//! by the config file that stores it and the dashboard that counts down on it.
//!
//! The bound used to be enforced by a `validate_refresh_interval` free function
//! called at six places — and, in the TUI, by a *seventh* path that silently
//! clamped instead of rejecting, so the same invariant had two policies. A
//! [`RefreshInterval`] can only be built by [`RefreshInterval::from_secs`],
//! which rejects, or [`RefreshInterval::clamping`], which is explicit about
//! doing the other thing. Once built it is in range, and nothing downstream
//! re-checks.

use std::{fmt, time::Duration};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

/// Fast enough to watch a room react to a window opening, without hammering a
/// device that takes a measurement every few seconds anyway.
pub const MIN_SECS: u64 = 5;
/// An hour. Past this the dashboard is showing history, not a live reading.
pub const MAX_SECS: u64 = 3600;
pub const DEFAULT_SECS: u64 = 30;

/// How much one press of `+`/`-` moves the interval.
const STEP: Duration = Duration::from_secs(5);

/// A refresh interval guaranteed to be between [`MIN_SECS`] and [`MAX_SECS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RefreshInterval(Duration);

#[derive(Debug, Error, PartialEq, Eq)]
#[error("refresh interval must be between {MIN_SECS} and {MAX_SECS} seconds, got {0}")]
pub struct OutOfRange(pub u64);

impl RefreshInterval {
    pub const MIN: Self = Self(Duration::from_secs(MIN_SECS));
    pub const MAX: Self = Self(Duration::from_secs(MAX_SECS));
    pub const DEFAULT: Self = Self(Duration::from_secs(DEFAULT_SECS));

    /// Rejects an out-of-range value. Use this wherever the number came from a
    /// person — a command argument, a config file, a typed field — so they are
    /// told their value was not accepted rather than left guessing why the
    /// dashboard is refreshing at some other rate.
    pub fn from_secs(seconds: u64) -> Result<Self, OutOfRange> {
        if (MIN_SECS..=MAX_SECS).contains(&seconds) {
            Ok(Self(Duration::from_secs(seconds)))
        } else {
            Err(OutOfRange(seconds))
        }
    }

    /// Pulls a duration into range instead of rejecting it.
    ///
    /// Only for values that are already the program's own — the `+`/`-` keys
    /// walking off the end of the range, or a duration reconstructed from an
    /// interval that was validated earlier. Never for user input.
    pub fn clamping(interval: Duration) -> Self {
        Self(interval.clamp(Self::MIN.0, Self::MAX.0))
    }

    pub const fn as_duration(self) -> Duration {
        self.0
    }

    pub const fn as_secs(self) -> u64 {
        self.0.as_secs()
    }

    /// One `+` press: a step longer, stopping at [`RefreshInterval::MAX`].
    pub fn longer(self) -> Self {
        Self::clamping(self.0.saturating_add(STEP))
    }

    /// One `-` press: a step shorter, stopping at [`RefreshInterval::MIN`].
    pub fn shorter(self) -> Self {
        Self::clamping(self.0.saturating_sub(STEP))
    }
}

impl Default for RefreshInterval {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for RefreshInterval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.as_secs())
    }
}

/// Stored as a plain number of seconds, because that is what the config file
/// has always held and the AirGradient desktop app reads the same key.
impl Serialize for RefreshInterval {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_secs().serialize(serializer)
    }
}

/// Deserializing enforces the bound, so a hand-edited file cannot put an
/// out-of-range interval into a `Config` at all. The tolerant reader in
/// `config::lossy` handles that case separately, by warning and defaulting.
impl<'de> Deserialize<'de> for RefreshInterval {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let seconds = u64::deserialize(deserializer)?;
        Self::from_secs(seconds).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_range_and_rejects_either_side_of_it() {
        assert_eq!(
            RefreshInterval::from_secs(MIN_SECS),
            Ok(RefreshInterval::MIN)
        );
        assert_eq!(
            RefreshInterval::from_secs(MAX_SECS),
            Ok(RefreshInterval::MAX)
        );
        assert_eq!(RefreshInterval::from_secs(30).map(|i| i.as_secs()), Ok(30));

        assert_eq!(
            RefreshInterval::from_secs(MIN_SECS - 1),
            Err(OutOfRange(MIN_SECS - 1))
        );
        assert_eq!(
            RefreshInterval::from_secs(MAX_SECS + 1),
            Err(OutOfRange(MAX_SECS + 1))
        );
        assert_eq!(RefreshInterval::from_secs(0), Err(OutOfRange(0)));
    }

    #[test]
    fn clamping_pulls_into_range_rather_than_failing() {
        assert_eq!(
            RefreshInterval::clamping(Duration::from_secs(0)),
            RefreshInterval::MIN
        );
        assert_eq!(
            RefreshInterval::clamping(Duration::from_secs(99_999)),
            RefreshInterval::MAX
        );
        assert_eq!(
            RefreshInterval::clamping(Duration::from_secs(45)).as_secs(),
            45
        );
    }

    #[test]
    fn steps_stop_at_the_bounds_instead_of_wrapping() {
        assert_eq!(RefreshInterval::MAX.longer(), RefreshInterval::MAX);
        assert_eq!(RefreshInterval::MIN.shorter(), RefreshInterval::MIN);

        let thirty = RefreshInterval::from_secs(30).unwrap();
        assert_eq!(thirty.longer().as_secs(), 35);
        assert_eq!(thirty.shorter().as_secs(), 25);
        assert_eq!(thirty.longer().shorter(), thirty);
    }

    #[test]
    fn round_trips_through_json_as_a_bare_number_of_seconds() {
        let interval = RefreshInterval::from_secs(45).unwrap();
        let json = serde_json::to_string(&interval).unwrap();

        assert_eq!(json, "45");
        assert_eq!(
            serde_json::from_str::<RefreshInterval>(&json).unwrap(),
            interval
        );
    }

    #[test]
    fn deserializing_enforces_the_bound() {
        let error = serde_json::from_str::<RefreshInterval>("99999")
            .expect_err("an out-of-range interval should not deserialize");

        assert!(error.to_string().contains("must be between"), "{error}");
    }
}
