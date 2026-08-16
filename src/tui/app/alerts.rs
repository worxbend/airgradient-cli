//! Noticing that the air got worse, and saying so.
//!
//! The dashboard shows every reading all the time, which is exactly why it
//! needs this: a number that has been on screen for an hour does not catch the
//! eye when it changes. An alert is raised on the *transition* into a
//! concerning band, so it fires when CO2 crosses into "Elevated" rather than
//! for every refresh it stays there — an alert that repeats every thirty
//! seconds is one nobody reads.
//!
//! Recovery is announced too. "Did opening the window help?" is the natural
//! next question, and answering it is what makes the first alert worth acting
//! on.

use std::{
    collections::VecDeque,
    time::{Duration, SystemTime},
};

use crate::sensors::{Metric, SensorSnapshot, Status, metrics};

use super::TuiApp;

/// How long one alert stays on screen before the next is shown.
const ALERT_DURATION: Duration = Duration::from_secs(6);

/// Alerts held back when several metrics turn at once.
///
/// A device coming back after an outage can report every metric as changed in
/// one go. Showing them one after another for six seconds each would take most
/// of a minute, so the queue is bounded and the worst news wins — see
/// [`TuiApp::raise_air_alerts`].
const MAX_QUEUED_ALERTS: usize = 4;

/// Which way a metric crossed the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// Reached a concerning band, or moved to a worse one.
    Worsened,
    /// Came back down out of the concerning bands.
    Recovered,
}

/// One thing worth interrupting the user about.
#[derive(Debug, Clone, PartialEq)]
pub struct AirAlert {
    pub metric_key: &'static str,
    pub kind: AlertKind,
    /// The band reached — what the toast is coloured by.
    pub status: Status,
    /// "PM2.5 is Unhealthy — 58.2 ug/m3"
    pub message: String,
    pub raised_at: SystemTime,
}

impl AirAlert {
    fn worsened(metric: &Metric, raised_at: SystemTime) -> Self {
        Self {
            metric_key: metric.key,
            kind: AlertKind::Worsened,
            status: metric.status,
            message: format!(
                "{} is {} — {}",
                metric.label,
                metric.status.label(),
                reading_text(metric)
            ),
            raised_at,
        }
    }

    fn recovered(metric: &Metric, raised_at: SystemTime) -> Self {
        Self {
            metric_key: metric.key,
            kind: AlertKind::Recovered,
            status: metric.status,
            message: format!(
                "{} back to {} — {}",
                metric.label,
                metric.status.label(),
                reading_text(metric)
            ),
            raised_at,
        }
    }
}

fn reading_text(metric: &Metric) -> String {
    let value = metric.formatted_value.as_deref().unwrap_or("--");

    if metric.unit.is_empty() {
        value.to_string()
    } else {
        format!("{value} {}", metric.unit)
    }
}

impl TuiApp {
    /// The alert currently on screen, if any.
    pub fn current_alert(&self) -> Option<&AirAlert> {
        self.air_alerts.front()
    }

    /// Compares a new reading against the one before it and queues an alert for
    /// every metric that crossed into — or back out of — a concerning band.
    ///
    /// Called with the snapshot that is *about to* become current, while
    /// `self.current_snapshot` still holds the one before it, so the comparison
    /// is between consecutive successful readings. The very first reading of a
    /// session raises nothing: with nothing to compare against, every
    /// concerning metric would alert at once, every launch.
    pub(super) fn raise_air_alerts(&mut self, next: &SensorSnapshot, at: SystemTime) {
        let Some(previous) = self.current_snapshot.as_ref() else {
            return;
        };

        let before = metrics(previous, None);
        let after = metrics(next, None);

        let mut raised: Vec<AirAlert> = after
            .iter()
            .filter_map(|metric| {
                let was = before.by_key(metric.key)?.status;
                alert_for(metric, was, at)
            })
            .collect();

        // Worst first, so a queue that has to be truncated keeps the news that
        // matters. `Recovered` alerts rank below every warning for the same
        // reason.
        raised.sort_by_key(|alert| {
            (
                alert.kind == AlertKind::Recovered,
                std::cmp::Reverse(alert.status.rank()),
            )
        });

        for alert in raised {
            if self.air_alerts.len() >= MAX_QUEUED_ALERTS {
                break;
            }
            self.air_alerts.push_back(alert);
        }
    }

    /// Drops the showing alert once its time is up, promoting the next.
    /// Returns whether anything changed, so the loop only repaints when it did.
    pub fn expire_due_alerts(&mut self, now: SystemTime) -> bool {
        let Some(current) = self.air_alerts.front() else {
            return false;
        };

        let elapsed = now
            .duration_since(current.raised_at)
            // A clock that jumped backwards leaves the alert up rather than
            // flashing it away; it will expire on the next tick regardless.
            .unwrap_or_default();

        if elapsed < ALERT_DURATION {
            return false;
        }

        self.air_alerts.pop_front();
        true
    }

    /// Clears every queued alert — used when the user has plainly seen them,
    /// such as on a manual refresh.
    pub fn dismiss_alerts(&mut self) -> bool {
        let had_any = !self.air_alerts.is_empty();
        self.air_alerts.clear();
        had_any
    }
}

/// The alert a single metric's change deserves, if any.
fn alert_for(metric: &Metric, was: Status, at: SystemTime) -> Option<AirAlert> {
    let now = metric.status;

    // A sensor dropping out or coming back is not the air changing.
    if now == Status::Unknown || was == Status::Unknown {
        return None;
    }

    if now.is_concerning() && now.rank() > was.rank() {
        return Some(AirAlert::worsened(metric, at));
    }

    if was.is_concerning() && !now.is_concerning() {
        return Some(AirAlert::recovered(metric, at));
    }

    None
}

/// The queue type, so `TuiApp` does not have to name it inline.
pub(super) type AlertQueue = VecDeque<AirAlert>;
