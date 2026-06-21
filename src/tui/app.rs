use std::{
    fmt,
    time::{Duration, SystemTime},
};

use reqwest::Client;
use url::Url;

use crate::{
    config::{MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS},
    device::{DeviceError, FetchSettings},
    sensors::{Metric, SensorSnapshot, metrics},
};

const REFRESH_STEP: Duration = Duration::from_secs(5);
const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);
const MAX_REFRESH_INTERVAL: Duration = Duration::from_secs(MAX_REFRESH_INTERVAL_SECS);

#[derive(Debug, Clone)]
pub struct TuiApp {
    pub current_snapshot: Option<SensorSnapshot>,
    pub previous_successful_snapshot: Option<SensorSnapshot>,
    pub last_fetch_duration: Option<Duration>,
    pub last_success_at: Option<SystemTime>,
    pub current_error: Option<String>,
    pub configured_url: Option<Url>,
    pub refresh_interval: Duration,
    pub fetch_settings: FetchSettings,
    pub fetch_client: Client,
}

impl TuiApp {
    pub fn new(
        configured_url: Option<Url>,
        refresh_interval: Duration,
        fetch_settings: FetchSettings,
    ) -> Result<Self, DeviceError> {
        let fetch_client = fetch_settings.client()?;
        Ok(Self::with_client(
            configured_url,
            refresh_interval,
            fetch_settings,
            fetch_client,
        ))
    }

    pub fn with_client(
        configured_url: Option<Url>,
        refresh_interval: Duration,
        fetch_settings: FetchSettings,
        fetch_client: Client,
    ) -> Self {
        Self {
            current_snapshot: None,
            previous_successful_snapshot: None,
            last_fetch_duration: None,
            last_success_at: None,
            current_error: None,
            configured_url,
            refresh_interval: clamp_refresh_interval(refresh_interval),
            fetch_settings,
            fetch_client,
        }
    }

    pub fn apply_success(
        &mut self,
        snapshot: SensorSnapshot,
        fetch_duration: Duration,
        success_at: SystemTime,
    ) {
        self.previous_successful_snapshot = self.current_snapshot.replace(snapshot);
        self.last_fetch_duration = Some(fetch_duration);
        self.last_success_at = Some(success_at);
        self.current_error = None;
    }

    pub fn apply_failure(&mut self, error: impl fmt::Display, fetch_duration: Duration) {
        self.last_fetch_duration = Some(fetch_duration);
        self.current_error = Some(error.to_string());
    }

    pub fn increase_refresh_interval(&mut self) {
        self.refresh_interval = self
            .refresh_interval
            .saturating_add(REFRESH_STEP)
            .min(MAX_REFRESH_INTERVAL);
    }

    pub fn decrease_refresh_interval(&mut self) {
        self.refresh_interval = self
            .refresh_interval
            .saturating_sub(REFRESH_STEP)
            .max(MIN_REFRESH_INTERVAL);
    }

    pub fn trend_baseline(&self) -> Option<&SensorSnapshot> {
        self.previous_successful_snapshot.as_ref()
    }

    pub fn metrics(&self) -> Vec<Metric> {
        self.current_snapshot
            .as_ref()
            .map(|snapshot| metrics(snapshot, self.trend_baseline()))
            .unwrap_or_default()
    }
}

fn clamp_refresh_interval(refresh_interval: Duration) -> Duration {
    refresh_interval.clamp(MIN_REFRESH_INTERVAL, MAX_REFRESH_INTERVAL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::Trend;

    fn app_with_refresh(refresh_interval: Duration) -> TuiApp {
        TuiApp::with_client(
            None,
            refresh_interval,
            FetchSettings::default(),
            Client::new(),
        )
    }

    fn snapshot(aqi: f64, co2: f64) -> SensorSnapshot {
        SensorSnapshot {
            aqi: Some(aqi),
            co2: Some(co2),
            ..SensorSnapshot::default()
        }
    }

    #[test]
    fn success_records_current_snapshot_and_fetch_metadata() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        let success_at = SystemTime::UNIX_EPOCH + Duration::from_secs(42);
        let current = snapshot(42.0, 612.0);

        app.apply_success(current.clone(), Duration::from_millis(128), success_at);

        assert_eq!(app.current_snapshot, Some(current));
        assert_eq!(app.previous_successful_snapshot, None);
        assert_eq!(app.last_fetch_duration, Some(Duration::from_millis(128)));
        assert_eq!(app.last_success_at, Some(success_at));
        assert_eq!(app.current_error, None);
    }

    #[test]
    fn failure_preserves_successful_snapshots_and_records_error() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        let first = snapshot(40.0, 600.0);
        let second = snapshot(45.0, 625.0);
        app.apply_success(
            first.clone(),
            Duration::from_millis(90),
            SystemTime::UNIX_EPOCH,
        );
        app.apply_success(
            second.clone(),
            Duration::from_millis(100),
            SystemTime::UNIX_EPOCH + Duration::from_secs(30),
        );

        app.apply_failure("request timed out", Duration::from_millis(250));

        assert_eq!(app.current_snapshot, Some(second));
        assert_eq!(app.previous_successful_snapshot, Some(first));
        assert_eq!(app.last_fetch_duration, Some(Duration::from_millis(250)));
        assert_eq!(app.current_error.as_deref(), Some("request timed out"));
    }

    #[test]
    fn trend_baseline_is_available_after_two_successes() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        app.apply_success(
            snapshot(40.0, 650.0),
            Duration::from_millis(50),
            SystemTime::UNIX_EPOCH,
        );
        app.apply_success(
            snapshot(55.0, 620.0),
            Duration::from_millis(60),
            SystemTime::UNIX_EPOCH + Duration::from_secs(30),
        );

        assert_eq!(
            app.trend_baseline().and_then(|snapshot| snapshot.aqi),
            Some(40.0)
        );
        let metrics = app.metrics();
        assert_eq!(metrics[0].trend, Trend::Up);
        assert_eq!(metrics[1].trend, Trend::Down);
    }

    #[test]
    fn refresh_interval_is_clamped_and_adjusted_within_bounds() {
        let mut low = app_with_refresh(Duration::from_secs(1));
        assert_eq!(low.refresh_interval, MIN_REFRESH_INTERVAL);
        low.decrease_refresh_interval();
        assert_eq!(low.refresh_interval, MIN_REFRESH_INTERVAL);
        low.increase_refresh_interval();
        assert_eq!(low.refresh_interval, Duration::from_secs(10));

        let mut high = app_with_refresh(Duration::from_secs(4_000));
        assert_eq!(high.refresh_interval, MAX_REFRESH_INTERVAL);
        high.increase_refresh_interval();
        assert_eq!(high.refresh_interval, MAX_REFRESH_INTERVAL);
        high.decrease_refresh_interval();
        assert_eq!(
            high.refresh_interval,
            Duration::from_secs(MAX_REFRESH_INTERVAL_SECS - 5)
        );
    }
}
