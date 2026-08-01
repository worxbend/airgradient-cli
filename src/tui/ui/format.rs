//! Value formatting shared by every TUI surface.
//!
//! Rendering modules stay layout-only by delegating "how does this read as
//! text" questions here, so a phrasing change lands in one place instead of
//! being duplicated across the compact and showcase layouts.

use std::time::{Duration, SystemTime};

use ratatui::text::Span;

use crate::{
    sensors::{MISSING_VALUE, Metric, SensorSnapshot, metrics},
    tui::app::TuiApp,
};

/// The metrics to render, falling back to an all-missing snapshot before the
/// first successful fetch so the dashboard has a stable shape from frame one
/// rather than popping into existence when data arrives.
pub(super) fn display_metrics(app: &TuiApp) -> Vec<Metric> {
    let app_metrics = app.metrics();
    if app_metrics.is_empty() {
        metrics(&SensorSnapshot::default(), None)
    } else {
        app_metrics
    }
}

pub(super) fn metric_by_key<'a>(metrics: &'a [Metric], key: &str) -> Option<&'a Metric> {
    metrics.iter().find(|metric| metric.key == key)
}

/// The AQI metric, which `sensors::metrics` always emits (missing readings
/// surface as an `Unknown` status rather than an absent entry).
pub(super) fn aqi_metric(metrics: &[Metric]) -> &Metric {
    metric_by_key(metrics, "aqi").expect("presentation metrics always include AQI")
}

pub(super) fn metric_value(metric: &Metric) -> &str {
    metric.formatted_value.as_deref().unwrap_or(MISSING_VALUE)
}

pub(super) fn metric_value_with_unit(metric: &Metric) -> String {
    if metric.unit.is_empty() {
        metric_value(metric).to_string()
    } else {
        format!("{} {}", metric_value(metric), metric.unit)
    }
}

/// The configured device URL without its trailing slash, or a prompt to set
/// one. Shared by the status bar and the showcase server-URL line.
pub(super) fn configured_url_text(app: &TuiApp) -> String {
    app.configured_url
        .as_ref()
        .map(|url| url.as_str().trim_end_matches('/').to_string())
        .unwrap_or_else(|| "no device URL configured".to_string())
}

/// One-line explanation of what the AQI reading currently represents. The
/// branch order matters: the most specific state (no config at all, then
/// retry-with-no-data) has to win over the general "refreshing" case.
pub(super) fn aqi_message(app: &TuiApp) -> &'static str {
    if app.configured_url.is_none() {
        "Set a device URL with config set-url or pass a URL override."
    } else if app.is_fetching && app.current_error.is_some() && app.current_snapshot.is_none() {
        "Retrying after a fetch failure; waiting for first successful reading."
    } else if app.is_fetching && app.current_error.is_some() {
        "Retrying after a fetch failure; showing the latest successful reading."
    } else if app.is_fetching && app.current_snapshot.is_none() {
        "Fetching the first air quality reading."
    } else if app.is_fetching {
        "Refreshing; showing the latest successful reading."
    } else if app.current_snapshot.is_none() {
        "Waiting for the first successful reading."
    } else {
        "Latest air quality reading."
    }
}

pub(super) fn fetch_status(app: &TuiApp) -> Span<'static> {
    let theme = app.theme;
    if app.configured_url.is_none() {
        return Span::styled("missing config", theme.error_style());
    }

    if app.is_fetching {
        if app.current_error.is_some() {
            return Span::styled("retrying", theme.error_style());
        }

        if app.current_snapshot.is_some() {
            return Span::styled("refreshing", theme.muted_style());
        }

        return Span::styled("fetching", theme.muted_style());
    }

    if app.current_error.is_some() {
        return Span::styled("fetch failed", theme.error_style());
    }

    if let Some(last_success_at) = app.last_success_at {
        let mut text = format!("updated {}", format_system_time_delta(last_success_at));
        if let Some(fetch_duration) = app.last_fetch_duration {
            text.push_str(&format!(" in {}", format_duration(fetch_duration)));
        }
        return Span::styled(text, theme.muted_style());
    }

    Span::styled("waiting for first fetch", theme.muted_style())
}

/// How long ago `time` was, in human units. A `time` in the future (clock
/// skew, or a reading stamped between frames) reads as "just now" rather than
/// producing a negative duration.
pub(super) fn format_system_time_delta(time: SystemTime) -> String {
    match SystemTime::now().duration_since(time) {
        Ok(elapsed) => format!("{} ago", format_duration(elapsed)),
        Err(_) => "just now".to_string(),
    }
}

/// Coarse human duration: sub-second values keep milliseconds, everything
/// else drops to the two largest non-zero units so the status bar stays a
/// fixed, glanceable width.
pub(super) fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds == 0 {
        return format!("{}ms", duration.as_millis());
    }

    if seconds < 60 {
        return format!("{seconds}s");
    }

    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    if minutes < 60 {
        return format_two_units(minutes, 'm', remaining_seconds, 's');
    }

    format_two_units(minutes / 60, 'h', minutes % 60, 'm')
}

fn format_two_units(major: u64, major_unit: char, minor: u64, minor_unit: char) -> String {
    if minor == 0 {
        format!("{major}{major_unit}")
    } else {
        format!("{major}{major_unit} {minor}{minor_unit}")
    }
}
