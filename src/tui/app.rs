use std::{
    collections::VecDeque,
    fmt,
    time::{Duration, SystemTime},
};

use url::Url;

use crate::{
    config::{MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS},
    sensors::{Metric, SensorSnapshot, metrics},
};

const REFRESH_STEP: Duration = Duration::from_secs(5);
const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);
const MAX_REFRESH_INTERVAL: Duration = Duration::from_secs(MAX_REFRESH_INTERVAL_SECS);
const MAX_READING_HISTORY: usize = 48;

#[derive(Debug, Clone)]
pub struct TuiApp {
    pub current_snapshot: Option<SensorSnapshot>,
    pub previous_successful_snapshot: Option<SensorSnapshot>,
    reading_history: VecDeque<SensorSnapshot>,
    pub last_fetch_duration: Option<Duration>,
    pub last_success_at: Option<SystemTime>,
    pub current_error: Option<String>,
    pub configured_url: Option<Url>,
    pub refresh_interval: Duration,
    pub is_fetching: bool,
}

impl TuiApp {
    pub fn new(configured_url: Option<Url>, refresh_interval: Duration) -> Self {
        Self {
            current_snapshot: None,
            previous_successful_snapshot: None,
            reading_history: VecDeque::new(),
            last_fetch_duration: None,
            last_success_at: None,
            current_error: None,
            configured_url,
            refresh_interval: clamp_refresh_interval(refresh_interval),
            is_fetching: false,
        }
    }

    pub fn begin_fetch(&mut self) {
        self.is_fetching = self.configured_url.is_some();
    }

    pub fn finish_fetch_success(
        &mut self,
        snapshot: SensorSnapshot,
        fetch_duration: Duration,
        success_at: SystemTime,
    ) {
        self.is_fetching = false;
        self.remember_successful_snapshot(snapshot.clone());
        self.previous_successful_snapshot = self.current_snapshot.replace(snapshot);
        self.last_fetch_duration = Some(fetch_duration);
        self.last_success_at = Some(success_at);
        self.current_error = None;
    }

    pub fn finish_fetch_failure(&mut self, error: impl fmt::Display, fetch_duration: Duration) {
        self.is_fetching = false;
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

    pub fn successful_snapshots(&self) -> impl Iterator<Item = &SensorSnapshot> {
        self.reading_history.iter()
    }

    fn remember_successful_snapshot(&mut self, snapshot: SensorSnapshot) {
        self.reading_history.push_back(snapshot);
        while self.reading_history.len() > MAX_READING_HISTORY {
            self.reading_history.pop_front();
        }
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
        TuiApp::new(None, refresh_interval)
    }

    fn app_with_url() -> TuiApp {
        TuiApp::new(
            Some(Url::parse("http://192.168.1.201/").expect("url should parse")),
            Duration::from_secs(30),
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

        app.begin_fetch();
        app.finish_fetch_success(current.clone(), Duration::from_millis(128), success_at);

        assert_eq!(app.current_snapshot, Some(current));
        assert_eq!(app.previous_successful_snapshot, None);
        assert_eq!(app.successful_snapshots().count(), 1);
        assert_eq!(app.last_fetch_duration, Some(Duration::from_millis(128)));
        assert_eq!(app.last_success_at, Some(success_at));
        assert_eq!(app.current_error, None);
        assert!(!app.is_fetching);
    }

    #[test]
    fn failure_preserves_successful_snapshots_and_records_error() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        let first = snapshot(40.0, 600.0);
        let second = snapshot(45.0, 625.0);
        app.finish_fetch_success(
            first.clone(),
            Duration::from_millis(90),
            SystemTime::UNIX_EPOCH,
        );
        app.finish_fetch_success(
            second.clone(),
            Duration::from_millis(100),
            SystemTime::UNIX_EPOCH + Duration::from_secs(30),
        );

        app.begin_fetch();
        app.finish_fetch_failure("request timed out", Duration::from_millis(250));

        assert_eq!(app.current_snapshot, Some(second));
        assert_eq!(app.previous_successful_snapshot, Some(first));
        assert_eq!(app.last_fetch_duration, Some(Duration::from_millis(250)));
        assert_eq!(app.current_error.as_deref(), Some("request timed out"));
        assert!(!app.is_fetching);
    }

    #[test]
    fn success_history_keeps_recent_readings_for_terminal_traces() {
        let mut app = app_with_refresh(Duration::from_secs(30));

        for index in 0_u32..60 {
            app.finish_fetch_success(
                snapshot(f64::from(index), 600.0 + f64::from(index)),
                Duration::from_millis(50),
                SystemTime::UNIX_EPOCH + Duration::from_secs(index.into()),
            );
        }

        let history = app.successful_snapshots().collect::<Vec<_>>();
        assert_eq!(history.len(), MAX_READING_HISTORY);
        assert_eq!(
            history.first().and_then(|snapshot| snapshot.aqi),
            Some(12.0)
        );
        assert_eq!(history.last().and_then(|snapshot| snapshot.aqi), Some(59.0));
    }

    #[test]
    fn trend_baseline_is_available_after_two_successes() {
        let mut app = app_with_refresh(Duration::from_secs(30));
        app.finish_fetch_success(
            snapshot(40.0, 650.0),
            Duration::from_millis(50),
            SystemTime::UNIX_EPOCH,
        );
        app.finish_fetch_success(
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

    #[test]
    fn begin_fetch_only_sets_pending_when_url_is_configured() {
        let mut missing_url = TuiApp::new(None, Duration::from_secs(30));
        missing_url.begin_fetch();
        assert!(!missing_url.is_fetching);

        let mut configured = app_with_url();
        configured.begin_fetch();
        assert!(configured.is_fetching);
    }

    #[test]
    fn finish_fetch_success_and_failure_clear_pending_state() {
        let mut success = app_with_url();
        success.begin_fetch();
        assert!(success.is_fetching);
        success.finish_fetch_success(
            snapshot(42.0, 612.0),
            Duration::from_millis(128),
            SystemTime::UNIX_EPOCH,
        );
        assert!(!success.is_fetching);

        let mut failure = app_with_url();
        failure.begin_fetch();
        assert!(failure.is_fetching);
        failure.finish_fetch_failure("request timed out", Duration::from_millis(250));
        assert!(!failure.is_fetching);
    }

    #[test]
    fn retry_preserves_previous_error_until_a_new_result_arrives() {
        let mut app = app_with_url();
        app.finish_fetch_failure("request timed out", Duration::from_millis(250));

        app.begin_fetch();

        assert!(app.is_fetching);
        assert_eq!(app.current_error.as_deref(), Some("request timed out"));

        app.finish_fetch_success(
            snapshot(42.0, 612.0),
            Duration::from_millis(128),
            SystemTime::UNIX_EPOCH,
        );

        assert!(!app.is_fetching);
        assert_eq!(app.current_error, None);
    }

    #[test]
    fn failure_after_retry_replaces_previous_error_and_retains_last_success() {
        let mut app = app_with_url();
        let successful = snapshot(42.0, 612.0);
        app.finish_fetch_success(
            successful.clone(),
            Duration::from_millis(128),
            SystemTime::UNIX_EPOCH,
        );
        app.finish_fetch_failure("request timed out", Duration::from_millis(250));

        app.begin_fetch();
        app.finish_fetch_failure("connection refused", Duration::from_millis(75));

        assert_eq!(app.current_snapshot, Some(successful));
        assert_eq!(app.current_error.as_deref(), Some("connection refused"));
        assert_eq!(app.last_fetch_duration, Some(Duration::from_millis(75)));
        assert!(!app.is_fetching);
    }
}
