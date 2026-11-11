//! Waybar `custom` module JSON for one reading.
//!
//! The bar asks for `return-type: json`, so stdout is one compact object and
//! nothing else. Waybar 0.16 fills only `text`, `alt`, `icon`, and
//! `percentage` in its own `format`. Metric placeholders are expanded here,
//! into `text`, before Waybar sees them. Status comes from the metric rows
//! [`crate::sensors::metrics`] already classified. Comfort scales have no
//! breakpoints, and they choose the CSS class only when the template names no
//! hazard metric. This module does not record history and it does not decide
//! whether a failed fetch exits 0; the command layer does.

use std::time::SystemTime;

use serde::Serialize;

use crate::output::OutputMetadata;
use crate::sensors::{Metrics, SensorSnapshot, Status, metrics};

const METRIC_SEPARATOR: &str = " · ";
const STALE_SUFFIX: &str = " · stale";
const STALE_CLASS: &str = "stale";
const OFFLINE_TEXT: &str = "offline";
const OFFLINE_CLASS: &str = "error";

#[derive(Serialize)]
struct WaybarOutput {
    text: String,
    tooltip: String,
    class: WaybarClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    percentage: Option<i64>,
}

/// A live reading is one class. A stale one is the hazard class plus `stale`,
/// because Waybar applies every entry. `error` is not a sensor status.
#[derive(Serialize)]
#[serde(untagged)]
enum WaybarClass {
    Status(Status),
    Offline(&'static str),
    Stale((Status, &'static str)),
}

pub fn render_live(snapshot: &SensorSnapshot, metadata: OutputMetadata<'_>) -> String {
    render_live_formatted(snapshot, metadata, None)
        .expect("the built-in waybar text is not a template")
}

/// `template` is a `--waybar-format` string. `None` is the full reading line.
pub fn render_live_formatted(
    snapshot: &SensorSnapshot,
    metadata: OutputMetadata<'_>,
    template: Option<&str>,
) -> Result<String, WaybarFormatError> {
    let rows = metrics(snapshot, None);
    let view = bar_view(&rows, template)?;
    Ok(encode(&WaybarOutput {
        text: view.text,
        tooltip: live_tooltip(metadata.device_url, &rows),
        class: WaybarClass::Status(view.class),
        percentage: view.percentage,
    }))
}

pub fn render_offline(error: &str) -> String {
    encode(&WaybarOutput {
        text: OFFLINE_TEXT.to_owned(),
        tooltip: error.to_owned(),
        class: WaybarClass::Offline(OFFLINE_CLASS),
        percentage: None,
    })
}

pub fn render_stale(
    snapshot: &SensorSnapshot,
    metadata: OutputMetadata<'_>,
    error: &str,
    age: &str,
) -> String {
    render_stale_formatted(snapshot, metadata, error, age, None)
        .expect("the built-in waybar text is not a template")
}

pub fn render_stale_formatted(
    snapshot: &SensorSnapshot,
    metadata: OutputMetadata<'_>,
    error: &str,
    age: &str,
    template: Option<&str>,
) -> Result<String, WaybarFormatError> {
    let rows = metrics(snapshot, None);
    let view = bar_view(&rows, template)?;
    let mut text = view.text;
    text.push_str(STALE_SUFFIX);
    Ok(encode(&WaybarOutput {
        text,
        tooltip: stale_tooltip(error, age, metadata.device_url, &rows),
        class: WaybarClass::Stale((view.class, STALE_CLASS)),
        percentage: view.percentage,
    }))
}

/// Reject a template before a fetch. Rendering parses it again.
pub fn parse_format(template: &str) -> Result<(), WaybarFormatError> {
    parse_template(template).map(|_| ())
}

#[derive(Debug, PartialEq, Eq)]
pub enum WaybarFormatError {
    UnknownPlaceholder(String),
    UnclosedPlaceholder,
}

impl std::fmt::Display for WaybarFormatError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPlaceholder(name) => write!(
                formatter,
                "unknown waybar placeholder `{{{name}}}`. Known placeholders: {KNOWN_PLACEHOLDERS}"
            ),
            Self::UnclosedPlaceholder => {
                write!(formatter, "waybar format has an unmatched `{{` or `}}`")
            }
        }
    }
}

impl std::error::Error for WaybarFormatError {}

const KNOWN_PLACEHOLDERS: &str =
    "aqi, co2, pm25, pm2p5, pm1, pm10, pm03, tvoc, nox, temp, humidity";

/// Alias, then the metric key [`crate::sensors::metrics`] already uses.
const PLACEHOLDERS: &[(&str, &str)] = &[
    ("aqi", "aqi"),
    ("co2", "co2"),
    ("pm25", "pm25"),
    ("pm2p5", "pm25"),
    ("pm2.5", "pm25"),
    ("pm1", "pm1"),
    ("pm1.0", "pm1"),
    ("pm10", "pm10"),
    ("pm03", "pm03_count"),
    ("pm03_count", "pm03_count"),
    ("tvoc", "tvoc"),
    ("nox", "nox"),
    ("temp", "temperature_c"),
    ("temperature", "temperature_c"),
    ("temperature_c", "temperature_c"),
    ("humidity", "humidity"),
];

struct BarView {
    text: String,
    class: Status,
    percentage: Option<i64>,
}

enum TemplatePart {
    Literal(String),
    Metric(&'static str),
}

struct Template {
    parts: Vec<TemplatePart>,
    keys: Vec<&'static str>,
}

fn bar_view(rows: &Metrics, template: Option<&str>) -> Result<BarView, WaybarFormatError> {
    let Some(template) = template else {
        return Ok(BarView {
            text: bar_text(rows),
            class: hazard_status(rows),
            percentage: aqi_percentage(rows),
        });
    };

    let parsed = parse_template(template)?;
    let text = parsed
        .parts
        .iter()
        .map(|part| match part {
            TemplatePart::Literal(literal) => literal.clone(),
            TemplatePart::Metric(key) => rows
                .by_key(key)
                .expect("placeholder keys come from the metric table")
                .reading_text(),
        })
        .collect();
    Ok(BarView {
        text,
        class: status_for(rows, &parsed.keys),
        percentage: percentage_for(rows, &parsed.keys),
    })
}

fn parse_template(template: &str) -> Result<Template, WaybarFormatError> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut keys = Vec::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                push_literal(&mut parts, &mut literal);
                let mut name = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    name.push(next);
                }
                if !closed {
                    return Err(WaybarFormatError::UnclosedPlaceholder);
                }
                let key = canonical_key(&name)?;
                if !keys.contains(&key) {
                    keys.push(key);
                }
                parts.push(TemplatePart::Metric(key));
            }
            '}' => return Err(WaybarFormatError::UnclosedPlaceholder),
            other => literal.push(other),
        }
    }
    push_literal(&mut parts, &mut literal);
    Ok(Template { parts, keys })
}

fn push_literal(parts: &mut Vec<TemplatePart>, literal: &mut String) {
    if !literal.is_empty() {
        parts.push(TemplatePart::Literal(std::mem::take(literal)));
    }
}

fn canonical_key(name: &str) -> Result<&'static str, WaybarFormatError> {
    PLACEHOLDERS
        .iter()
        .find(|(alias, _)| *alias == name)
        .map(|(_, key)| *key)
        .ok_or_else(|| WaybarFormatError::UnknownPlaceholder(name.to_owned()))
}

/// Worst status among the named metrics. A hazard metric in the template keeps
/// temperature and humidity from painting the bar. A template that names only
/// those two uses their comfort status, so a temperature-only module can still
/// be colored.
fn status_for(rows: &Metrics, keys: &[&str]) -> Status {
    let referenced: Vec<&crate::sensors::Metric> =
        keys.iter().filter_map(|key| rows.by_key(key)).collect();
    let hazards: Vec<&crate::sensors::Metric> = referenced
        .iter()
        .copied()
        .filter(|metric| !metric.scale.breakpoints().is_empty())
        .collect();
    let pool = if hazards.is_empty() {
        referenced
    } else {
        hazards
    };
    pool.iter()
        .map(|metric| metric.status)
        .max_by_key(|status| status.rank())
        .unwrap_or(Status::Unknown)
}

fn percentage_for(rows: &Metrics, keys: &[&str]) -> Option<i64> {
    if keys.contains(&"aqi") {
        aqi_percentage(rows)
    } else {
        None
    }
}

/// How old a cached reading is, for the stale tooltip.
///
/// One unit, on purpose: the tooltip also lists every metric, so the age line
/// stays short. A stamp in the future (clock step) reads as "just now" rather
/// than a negative age, and the `stale` class still says it is not a new fetch.
pub(crate) fn format_reading_age(reading_at: SystemTime, now: SystemTime) -> String {
    let Ok(elapsed) = now.duration_since(reading_at) else {
        return "just now".to_string();
    };

    let seconds = elapsed.as_secs();
    let body = if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 60 * 60 {
        format!("{}m", seconds / 60)
    } else if seconds < 24 * 60 * 60 {
        format!("{}h", seconds / 3_600)
    } else {
        format!("{}d", seconds / 86_400)
    };
    format!("{body} ago")
}

fn encode(output: &WaybarOutput) -> String {
    serde_json::to_string(output).expect("waybar output is serializable")
}

fn bar_text(rows: &Metrics) -> String {
    rows.iter()
        .map(|metric| format!("{} {}", metric.label, metric.reading_text()))
        .collect::<Vec<_>>()
        .join(METRIC_SEPARATOR)
}

fn live_tooltip(device_url: Option<&str>, rows: &Metrics) -> String {
    let mut lines = Vec::new();
    if let Some(device_url) = device_url {
        lines.push(device_url.to_owned());
    }
    lines.extend(metric_lines(rows));
    lines.join("\n")
}

fn stale_tooltip(error: &str, age: &str, device_url: Option<&str>, rows: &Metrics) -> String {
    let mut lines = vec![error.to_owned(), age.to_owned()];
    if let Some(device_url) = device_url {
        lines.push(device_url.to_owned());
    }
    lines.extend(metric_lines(rows));
    lines.join("\n")
}

fn metric_lines(rows: &Metrics) -> impl Iterator<Item = String> + '_ {
    rows.iter().map(|metric| {
        format!(
            "{} {} {}",
            metric.label,
            metric.reading_text(),
            metric.status.label()
        )
    })
}

/// Worst hazard status. Empty breakpoints are the comfort scales, and a warm
/// room must not set the bar colour.
fn hazard_status(rows: &Metrics) -> Status {
    rows.iter()
        .filter(|metric| !metric.scale.breakpoints().is_empty())
        .map(|metric| metric.status)
        .max_by_key(|status| status.rank())
        .unwrap_or(Status::Unknown)
}

fn aqi_percentage(rows: &Metrics) -> Option<i64> {
    let aqi = rows.aqi().value.filter(|reading| reading.is_finite())?;
    Some(aqi.round() as i64)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use serde_json::{Value, json};

    fn parse(rendered: &str) -> Value {
        serde_json::from_str(rendered).expect("waybar output should be JSON")
    }

    fn full_snapshot() -> SensorSnapshot {
        SensorSnapshot {
            aqi: Some(42.0),
            co2: Some(2500.0),
            pm25: Some(7.4),
            pm1: Some(1.2),
            pm10: Some(8.0),
            pm03_count: Some(100.0),
            tvoc: Some(50.0),
            nox: Some(1.0),
            temperature_c: Some(22.0),
            humidity: Some(45.0),
        }
    }

    #[test]
    fn renders_a_full_reading_as_one_compact_object() {
        let rendered = render_live(
            &full_snapshot(),
            OutputMetadata {
                device_url: Some("http://monitor.local/"),
                ..OutputMetadata::default()
            },
        );
        let output = parse(&rendered);

        assert_eq!(
            output["text"],
            "AQI 42 · CO2 2500 ppm · PM2.5 7.4 ug/m3 · PM1.0 1.2 ug/m3 · PM10 8.0 ug/m3 · PM0.3 count 100 count/dL · TVOC 50 index · NOx 1 index · Temperature 22.0 C · Humidity 45.0 %"
        );
        assert_eq!(output["class"], "very_unhealthy");
        assert!(output["class"].is_string());
        assert_eq!(output["percentage"], 42);
        assert_eq!(
            output["tooltip"],
            "http://monitor.local/\nAQI 42 Good\nCO2 2500 ppm Very unhealthy\nPM2.5 7.4 ug/m3 Good\nPM1.0 1.2 ug/m3 Good\nPM10 8.0 ug/m3 Good\nPM0.3 count 100 count/dL Good\nTVOC 50 index Good\nNOx 1 index Good\nTemperature 22.0 C Good\nHumidity 45.0 % Good"
        );
        assert!(
            rendered
                .chars()
                .all(|ch| !('\u{e000}'..='\u{f8ff}').contains(&ch)),
            "waybar text must not use Nerd Font private-use icons"
        );
        assert!(!rendered.contains('↑') && !rendered.contains('↓'));
    }

    #[test]
    fn a_missing_sensor_stays_the_missing_token() {
        let output = parse(&render_live(
            &SensorSnapshot {
                aqi: Some(10.0),
                ..SensorSnapshot::default()
            },
            OutputMetadata::default(),
        ));

        let text = output["text"].as_str().expect("text");
        assert!(text.contains("PM2.5 -- ug/m3"), "{text}");
        assert!(text.contains("AQI 10"), "{text}");
        assert!(!text.contains("PM2.5 0"), "{text}");
        assert_eq!(output["percentage"], 10);
    }

    #[test]
    fn a_template_prints_only_the_named_metrics_and_colors_from_those() {
        let output = parse(
            &render_live_formatted(&full_snapshot(), OutputMetadata::default(), Some("{aqi}"))
                .expect("aqi is a known placeholder"),
        );

        assert_eq!(output["text"], "42");
        assert_eq!(output["class"], "good");
        assert_eq!(output["percentage"], 42);

        let mixed = parse(
            &render_live_formatted(
                &full_snapshot(),
                OutputMetadata::default(),
                Some("{temp} {pm2p5}"),
            )
            .expect("temp and pm2p5 are known placeholders"),
        );
        assert_eq!(mixed["text"], "22.0 C 7.4 ug/m3");
        assert_eq!(mixed["class"], "good");
        assert!(mixed.get("percentage").is_none());
    }

    #[test]
    fn a_template_keeps_a_missing_sensor_as_the_missing_token() {
        let output = parse(
            &render_live_formatted(
                &SensorSnapshot {
                    aqi: Some(10.0),
                    ..SensorSnapshot::default()
                },
                OutputMetadata::default(),
                Some("PM {pm2p5}"),
            )
            .expect("pm2p5 is a known placeholder"),
        );

        assert_eq!(output["text"], "PM -- ug/m3");
        assert!(output.get("percentage").is_none());
    }

    #[test]
    fn a_comfort_only_template_uses_the_comfort_status() {
        let temperature = parse(
            &render_live_formatted(
                &SensorSnapshot {
                    aqi: Some(42.0),
                    temperature_c: Some(40.0),
                    ..SensorSnapshot::default()
                },
                OutputMetadata::default(),
                Some("{temp}"),
            )
            .expect("temp is a known placeholder"),
        );
        assert_eq!(temperature["text"], "40.0 C");
        assert_eq!(temperature["class"], "elevated");

        let mixed = parse(
            &render_live_formatted(
                &SensorSnapshot {
                    aqi: Some(42.0),
                    temperature_c: Some(40.0),
                    ..SensorSnapshot::default()
                },
                OutputMetadata::default(),
                Some("{aqi} {temp}"),
            )
            .expect("aqi and temp are known placeholders"),
        );
        assert_eq!(mixed["class"], "good");
        assert_eq!(mixed["percentage"], 42);
    }

    #[test]
    fn a_template_on_a_stale_reading_drops_the_metrics_it_does_not_name() {
        let output = parse(
            &render_stale_formatted(
                &SensorSnapshot {
                    aqi: Some(80.0),
                    co2: Some(2100.0),
                    ..SensorSnapshot::default()
                },
                OutputMetadata::default(),
                "device down",
                "2m ago",
                Some("{aqi}"),
            )
            .expect("aqi is a known placeholder"),
        );

        assert_eq!(output["text"], "80 · stale");
        assert_eq!(output["class"], json!(["moderate", "stale"]));
        assert_eq!(output["percentage"], 80);
    }

    #[test]
    fn an_unknown_or_unclosed_placeholder_is_rejected() {
        assert!(matches!(
            parse_format("{nope}"),
            Err(WaybarFormatError::UnknownPlaceholder(name)) if name == "nope"
        ));
        assert_eq!(
            parse_format("{aqi"),
            Err(WaybarFormatError::UnclosedPlaceholder)
        );
        let escaped =
            render_live_formatted(&full_snapshot(), OutputMetadata::default(), Some("{{aqi}}"))
                .expect("escaped braces are literal");
        assert!(
            escaped.contains("\"text\":\"{aqi}\""),
            "escaped braces should stay literal, got {escaped}"
        );
    }

    #[test]
    fn comfort_does_not_set_the_class() {
        let output = parse(&render_live(
            &SensorSnapshot {
                aqi: Some(42.0),
                temperature_c: Some(40.0),
                humidity: Some(5.0),
                ..SensorSnapshot::default()
            },
            OutputMetadata::default(),
        ));

        assert_eq!(output["class"], "good");
        let text = output["text"].as_str().expect("text");
        assert!(text.contains("Temperature 40.0 C"), "{text}");
        assert!(text.contains("Humidity 5.0 %"), "{text}");
    }

    #[test]
    fn percentage_is_omitted_unless_aqi_is_a_finite_number() {
        let missing = parse(&render_live(
            &SensorSnapshot {
                aqi: None,
                pm25: Some(7.4),
                co2: Some(900.0),
                ..SensorSnapshot::default()
            },
            OutputMetadata::default(),
        ));
        assert!(missing.get("percentage").is_none());
        assert_eq!(missing["class"], "moderate");
        let text = missing["text"].as_str().expect("text");
        assert!(text.contains("AQI --"), "{text}");
        assert!(!text.contains("AQI 0"), "{text}");

        for reading in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let output = parse(&render_live(
                &SensorSnapshot {
                    aqi: Some(reading),
                    ..SensorSnapshot::default()
                },
                OutputMetadata::default(),
            ));
            assert!(
                output.get("percentage").is_none(),
                "non-finite AQI must not become a percentage, got {output}"
            );
        }
    }

    #[test]
    fn percentage_rounds_to_the_nearest_integer() {
        let output = parse(&render_live(
            &SensorSnapshot {
                aqi: Some(42.6),
                ..SensorSnapshot::default()
            },
            OutputMetadata::default(),
        ));
        assert_eq!(output["percentage"], 43);
    }

    #[test]
    fn offline_has_no_invented_readings() {
        let output = parse(&render_offline("device down"));
        assert_eq!(
            output,
            json!({
                "text": "offline",
                "tooltip": "device down",
                "class": "error"
            })
        );
    }

    #[test]
    fn stale_reading_keeps_the_old_numbers_and_marks_them() {
        let output = parse(&render_stale(
            &SensorSnapshot {
                aqi: Some(80.0),
                co2: Some(2100.0),
                ..SensorSnapshot::default()
            },
            OutputMetadata {
                device_url: Some("http://monitor.local/"),
                ..OutputMetadata::default()
            },
            "device down",
            "2m ago",
        ));

        let text = output["text"].as_str().expect("text");
        assert!(text.contains("AQI 80"), "{text}");
        assert!(text.contains("CO2 2100 ppm"), "{text}");
        assert!(text.ends_with(" · stale"), "{text}");
        assert_eq!(output["class"], json!(["very_unhealthy", "stale"]));
        assert_eq!(output["percentage"], 80);
        let tooltip = output["tooltip"].as_str().expect("tooltip");
        assert!(
            tooltip.starts_with("device down\n2m ago\nhttp://monitor.local/\n"),
            "{tooltip}"
        );
        assert!(!tooltip.to_ascii_lowercase().contains("success"));
    }

    #[test]
    fn reading_age_uses_one_coarse_unit_and_clamps_the_future() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let age =
            |seconds_ago: u64| format_reading_age(now - Duration::from_secs(seconds_ago), now);

        assert_eq!(age(0), "0s ago");
        assert_eq!(age(59), "59s ago");
        assert_eq!(age(60), "1m ago");
        assert_eq!(age(3_599), "59m ago");
        assert_eq!(age(3_600), "1h ago");
        assert_eq!(age(86_400), "1d ago");
        assert_eq!(
            format_reading_age(now + Duration::from_secs(1), now),
            "just now"
        );
    }
}
