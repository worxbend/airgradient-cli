use comfy_table::{Table, presets::ASCII_FULL};
use owo_colors::{OwoColorize, colors::css::Orange};

use crate::output::OutputMetadata;
use crate::sensors::{MISSING_VALUE, Metric, SensorSnapshot, Status, metrics};

pub fn render(
    snapshot: &SensorSnapshot,
    previous: Option<&SensorSnapshot>,
    metadata: OutputMetadata<'_>,
    no_color: bool,
) -> String {
    let metrics = metrics(snapshot, previous);
    let aqi = metrics
        .iter()
        .find(|metric| metric.key == "aqi")
        .expect("presentation metrics always include AQI");

    let mut output = String::new();
    output.push_str(&render_header(metadata));
    output.push('\n');
    output.push_str(&render_aqi_line(aqi, no_color));
    output.push_str("\n\n");
    output.push_str(&render_metric_table(&metrics, no_color));
    output.push('\n');
    output
}

fn render_header(metadata: OutputMetadata<'_>) -> String {
    let mut parts = Vec::new();

    if let Some(device_url) = metadata.device_url {
        parts.push(format!("Device: {device_url}"));
    }

    if let Some(last_update) = metadata.last_update {
        parts.push(format!("Updated: {last_update}"));
    }

    if let Some(fetch_duration) = metadata.fetch_duration {
        parts.push(format!("Fetch: {}", format_fetch_latency(fetch_duration)));
    }

    if parts.is_empty() {
        "AirGradient".to_string()
    } else {
        parts.join(" | ")
    }
}

fn render_aqi_line(metric: &Metric, no_color: bool) -> String {
    let value = metric.formatted_value.as_deref().unwrap_or(MISSING_VALUE);
    let status = metric.status.label();
    let line = format!("AQI {value} - {status}");
    colorize_status(&line, metric.status, no_color)
}

fn render_metric_table(metrics: &[Metric], no_color: bool) -> String {
    let mut table = Table::new();
    table
        .load_preset(ASCII_FULL)
        .set_header(["Metric", "Value", "Unit", "Status", "Trend"]);

    for metric in metrics {
        table.add_row([
            metric.label.to_string(),
            metric
                .formatted_value
                .clone()
                .unwrap_or_else(|| MISSING_VALUE.to_string()),
            metric.unit.to_string(),
            colorize_status(metric.status.label(), metric.status, no_color),
            metric.trend.display_symbol().to_string(),
        ]);
    }

    table.to_string()
}

fn colorize_status(text: &str, status: Status, no_color: bool) -> String {
    if no_color {
        return text.to_string();
    }

    match status {
        Status::Unknown => text.bright_black().to_string(),
        Status::Good => text.green().to_string(),
        Status::Moderate => text.yellow().to_string(),
        Status::Elevated => text.fg::<Orange>().to_string(),
        Status::Unhealthy => text.red().to_string(),
        Status::VeryUnhealthy => text.magenta().to_string(),
    }
}

/// Fetch latency at two-decimal precision.
///
/// Deliberately not the TUI's `ui::format::format_duration`, which rounds to
/// whole units for a status bar read at a glance. This output is scraped and
/// compared between runs, so a 1.20s and a 1.80s fetch must not both print as
/// "1s". The two agree below a second, where both print whole milliseconds.
fn format_fetch_latency(duration: std::time::Duration) -> String {
    let millis = duration.as_millis();
    if millis < 1_000 {
        format!("{millis}ms")
    } else {
        format!("{:.2}s", duration.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_missing_values_as_dashes() {
        let output = render(
            &SensorSnapshot::default(),
            None,
            OutputMetadata {
                device_url: Some("http://192.168.1.201"),
                last_update: Some("2026-06-21T12:00:00Z"),
                fetch_duration: Some(std::time::Duration::from_millis(42)),
            },
            true,
        );

        assert!(output.contains("Device: http://192.168.1.201"));
        assert!(output.contains("Updated: 2026-06-21T12:00:00Z"));
        assert!(output.contains("Fetch: 42ms"));
        assert!(output.contains("AQI -- - Unknown"));
        assert!(output.lines().any(|line| line.contains("| AQI")
            && line.contains("| --")
            && line.contains("| Unknown")));
    }

    #[test]
    fn no_color_suppresses_ansi_escapes() {
        let snapshot = SensorSnapshot {
            aqi: Some(42.0),
            ..SensorSnapshot::default()
        };

        let output = render(&snapshot, None, OutputMetadata::default(), true);

        assert!(!output.contains("\u{1b}["));
    }

    #[test]
    fn color_output_uses_ansi_escapes() {
        let snapshot = SensorSnapshot {
            aqi: Some(42.0),
            ..SensorSnapshot::default()
        };

        let output = render(&snapshot, None, OutputMetadata::default(), false);

        assert!(output.contains("\u{1b}["));
    }
}
