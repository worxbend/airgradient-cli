use serde::Serialize;
use serde_json::Value;

use crate::output::OutputMetadata;
use crate::sensors::{Metric, SensorSnapshot, metrics};

#[derive(Debug, Serialize)]
struct JsonOutput<'a> {
    device_url: Option<&'a str>,
    last_update: Option<&'a str>,
    fetch_duration_ms: Option<u128>,
    snapshot: &'a SensorSnapshot,
    metrics: Vec<JsonMetric>,
}

#[derive(Debug, Serialize)]
struct JsonMetric {
    key: &'static str,
    label: &'static str,
    unit: &'static str,
    value: Option<f64>,
    formatted_value: Option<String>,
    status: crate::sensors::Status,
    status_label: &'static str,
    trend: crate::sensors::Trend,
}

pub fn render_value(
    snapshot: &SensorSnapshot,
    previous: Option<&SensorSnapshot>,
    metadata: OutputMetadata<'_>,
) -> Value {
    let output = JsonOutput {
        device_url: metadata.device_url,
        last_update: metadata.last_update,
        fetch_duration_ms: metadata.fetch_duration.map(|duration| duration.as_millis()),
        snapshot,
        metrics: metrics(snapshot, previous)
            .into_iter()
            .map(JsonMetric::from)
            .collect(),
    };

    serde_json::to_value(output).expect("normalized sensor output is serializable")
}

pub fn render_pretty(
    snapshot: &SensorSnapshot,
    previous: Option<&SensorSnapshot>,
    metadata: OutputMetadata<'_>,
) -> serde_json::Result<String> {
    serde_json::to_string_pretty(&render_value(snapshot, previous, metadata))
}

impl From<Metric> for JsonMetric {
    fn from(metric: Metric) -> Self {
        Self {
            key: metric.key,
            label: metric.label,
            unit: metric.unit,
            value: metric.value,
            formatted_value: metric.formatted_value,
            status: metric.status,
            status_label: metric.status.label(),
            trend: metric.trend,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_missing_values_as_null() {
        let output = render_value(
            &SensorSnapshot::default(),
            None,
            OutputMetadata {
                device_url: Some("http://192.168.1.201"),
                last_update: Some("2026-06-21T12:00:00Z"),
                fetch_duration: Some(std::time::Duration::from_millis(42)),
            },
        );

        assert_eq!(output["device_url"], "http://192.168.1.201");
        assert_eq!(output["last_update"], "2026-06-21T12:00:00Z");
        assert_eq!(output["fetch_duration_ms"], 42);
        assert_eq!(output["snapshot"]["aqi"], Value::Null);
        assert_eq!(output["snapshot"]["pm25"], Value::Null);
        assert_eq!(output["metrics"][0]["key"], "aqi");
        assert_eq!(output["metrics"][0]["value"], Value::Null);
        assert_eq!(output["metrics"][0]["formatted_value"], Value::Null);
    }

    #[test]
    fn renders_same_metric_set_with_status_and_trend() {
        let current = SensorSnapshot {
            aqi: Some(42.0),
            co2: Some(650.0),
            ..SensorSnapshot::default()
        };
        let previous = SensorSnapshot {
            aqi: Some(45.0),
            co2: Some(650.0),
            ..SensorSnapshot::default()
        };

        let output = render_value(&current, Some(&previous), OutputMetadata::default());

        assert_eq!(output["metrics"].as_array().unwrap().len(), 10);
        assert_eq!(
            output["metrics"][0],
            json!({
                "key": "aqi",
                "label": "AQI",
                "unit": "",
                "value": 42.0,
                "formatted_value": "42",
                "status": "good",
                "status_label": "Good",
                "trend": "down"
            })
        );
        assert_eq!(output["metrics"][1]["trend"], "stable");
    }
}
