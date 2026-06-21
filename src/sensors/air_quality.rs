use serde::{Deserialize, Serialize};
use serde_json::Value;

const AQI_KEYS: &[&str] = &[
    "aqi",
    "pm02Aqi",
    "pm25Aqi",
    "pm2_5Aqi",
    "pm2_5_aqi",
    "usAqi",
    "us_aqi",
];
const CO2_KEYS: &[&str] = &["rco2", "co2", "co2Ppm", "co2_ppm"];
const PM25_KEYS: &[&str] = &["pm02", "pm2_5", "pm25", "pm2.5", "pm2_5_ugm3"];
const PM1_KEYS: &[&str] = &["pm01", "pm1", "pm1_0", "pm1.0", "pm1_0_ugm3"];
const PM10_KEYS: &[&str] = &["pm10", "pm10_0", "pm10.0", "pm10_ugm3"];
const PM03_COUNT_KEYS: &[&str] = &[
    "pm003Count",
    "pm03Count",
    "pm0_3Count",
    "pm0.3Count",
    "pm003_count",
    "pm0_3_count",
];
const TEMPERATURE_COMPENSATED_KEYS: &[&str] = &[
    "atmpCompensated",
    "temperatureCompensated",
    "temperature_compensated",
    "tempCompensated",
];
const TEMPERATURE_KEYS: &[&str] = &[
    "atmp",
    "temperature",
    "temperatureC",
    "temperature_c",
    "temp",
];
const HUMIDITY_COMPENSATED_KEYS: &[&str] = &[
    "rhumCompensated",
    "humidityCompensated",
    "humidity_compensated",
];
const HUMIDITY_KEYS: &[&str] = &["rhum", "humidity", "relativeHumidity", "relative_humidity"];
const TVOC_KEYS: &[&str] = &["tvocIndex", "tvoc", "vocIndex", "voc_index", "tvoc_index"];
const NOX_KEYS: &[&str] = &["noxIndex", "nox", "nox_index"];
const MAX_AQI: f64 = 500.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SensorSnapshot {
    pub aqi: Option<f64>,
    pub co2: Option<f64>,
    pub pm25: Option<f64>,
    pub pm1: Option<f64>,
    pub pm10: Option<f64>,
    pub pm03_count: Option<f64>,
    pub tvoc: Option<f64>,
    pub nox: Option<f64>,
    pub temperature_c: Option<f64>,
    pub humidity: Option<f64>,
}

impl SensorSnapshot {
    pub fn from_airgradient_payload(payload: &Value) -> Self {
        let pm25 = find_non_negative_number(payload, PM25_KEYS);
        let explicit_aqi = find_bounded_number(payload, AQI_KEYS, 0.0, MAX_AQI);

        Self {
            aqi: explicit_aqi.or_else(|| pm25.map(us_aqi_from_pm25)),
            co2: find_non_negative_number(payload, CO2_KEYS),
            pm25,
            pm1: find_non_negative_number(payload, PM1_KEYS),
            pm10: find_non_negative_number(payload, PM10_KEYS),
            pm03_count: find_non_negative_number(payload, PM03_COUNT_KEYS),
            tvoc: find_non_negative_number(payload, TVOC_KEYS),
            nox: find_non_negative_number(payload, NOX_KEYS),
            temperature_c: find_number(payload, TEMPERATURE_COMPENSATED_KEYS)
                .or_else(|| find_number(payload, TEMPERATURE_KEYS)),
            humidity: find_bounded_number(payload, HUMIDITY_COMPENSATED_KEYS, 0.0, 100.0)
                .or_else(|| find_bounded_number(payload, HUMIDITY_KEYS, 0.0, 100.0)),
        }
    }
}

pub fn parse_snapshot(payload: &Value) -> SensorSnapshot {
    SensorSnapshot::from_airgradient_payload(payload)
}

pub fn us_aqi_from_pm25(pm25: f64) -> f64 {
    if !pm25.is_finite() || pm25 < 0.0 {
        return 0.0;
    }

    let truncated = (pm25 * 10.0).floor() / 10.0;
    let breakpoints = [
        (0.0, 9.0, 0.0, 50.0),
        (9.1, 35.4, 51.0, 100.0),
        (35.5, 55.4, 101.0, 150.0),
        (55.5, 125.4, 151.0, 200.0),
        (125.5, 225.4, 201.0, 300.0),
        (225.5, 325.4, 301.0, 500.0),
    ];

    for (c_low, c_high, i_low, i_high) in breakpoints {
        if (c_low..=c_high).contains(&truncated) {
            return (((i_high - i_low) / (c_high - c_low)) * (truncated - c_low) + i_low).round();
        }
    }

    500.0
}

fn find_number(value: &Value, keys: &[&str]) -> Option<f64> {
    match value {
        Value::Object(map) => {
            for wanted in keys {
                if let Some(found) = map
                    .iter()
                    .find(|(key, _)| key_matches(key, wanted))
                    .and_then(|(_, value)| value_as_number(value))
                {
                    return Some(found);
                }
            }

            map.values().find_map(|child| {
                matches!(child, Value::Object(_) | Value::Array(_))
                    .then(|| find_number(child, keys))
                    .flatten()
            })
        }
        Value::Array(items) => items.iter().find_map(|child| find_number(child, keys)),
        _ => None,
    }
}

fn find_non_negative_number(value: &Value, keys: &[&str]) -> Option<f64> {
    find_number(value, keys).filter(|value| *value >= 0.0)
}

fn find_bounded_number(value: &Value, keys: &[&str], min: f64, max: f64) -> Option<f64> {
    find_number(value, keys).filter(|value| (min..=max).contains(value))
}

fn value_as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().filter(|value| value.is_finite()),
        Value::String(text) => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite()),
        _ => None,
    }
}

fn key_matches(actual: &str, expected: &str) -> bool {
    normalized_key(actual) == normalized_key(expected)
}

fn normalized_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_current_airgradient_sample_fields() {
        let payload = json!({
            "rco2": 612,
            "pm02": 7.4,
            "pm01": 4.2,
            "pm10": 11.8,
            "pm003Count": 1234,
            "atmpCompensated": 22.6,
            "rhumCompensated": 48.1,
            "tvocIndex": 83,
            "noxIndex": 2
        });

        let snapshot = parse_snapshot(&payload);

        assert_eq!(snapshot.co2, Some(612.0));
        assert_eq!(snapshot.pm25, Some(7.4));
        assert_eq!(snapshot.pm1, Some(4.2));
        assert_eq!(snapshot.pm10, Some(11.8));
        assert_eq!(snapshot.pm03_count, Some(1234.0));
        assert_eq!(snapshot.temperature_c, Some(22.6));
        assert_eq!(snapshot.humidity, Some(48.1));
        assert_eq!(snapshot.tvoc, Some(83.0));
        assert_eq!(snapshot.nox, Some(2.0));
    }

    #[test]
    fn searches_nested_values_and_parses_numeric_strings() {
        let payload = json!({
            "device": {
                "measurements": {
                    "co2": "805",
                    "pm2_5": "12.3",
                    "temperature": "21.5"
                }
            }
        });

        let snapshot = parse_snapshot(&payload);

        assert_eq!(snapshot.co2, Some(805.0));
        assert_eq!(snapshot.pm25, Some(12.3));
        assert_eq!(snapshot.temperature_c, Some(21.5));
    }

    #[test]
    fn preserves_missing_values_as_none() {
        let snapshot = parse_snapshot(&json!({ "rco2": 0 }));

        assert_eq!(snapshot.co2, Some(0.0));
        assert_eq!(snapshot.pm25, None);
        assert_eq!(snapshot.humidity, None);
    }

    #[test]
    fn computes_aqi_from_pm25_when_explicit_aqi_is_missing() {
        let snapshot = parse_snapshot(&json!({ "pm02": 12.1 }));

        assert_eq!(snapshot.aqi, Some(57.0));
    }

    #[test]
    fn treats_negative_pm25_as_missing_without_aqi_fallback() {
        let snapshot = parse_snapshot(&json!({ "pm02": -1.0 }));

        assert_eq!(snapshot.pm25, None);
        assert_eq!(snapshot.aqi, None);
    }

    #[test]
    fn treats_negative_pm1_as_missing() {
        let snapshot = parse_snapshot(&json!({ "pm01": -1.0 }));

        assert_eq!(snapshot.pm1, None);
    }

    #[test]
    fn treats_negative_pm10_as_missing() {
        let snapshot = parse_snapshot(&json!({ "pm10": -1.0 }));

        assert_eq!(snapshot.pm10, None);
    }

    #[test]
    fn treats_negative_pm03_count_as_missing() {
        let snapshot = parse_snapshot(&json!({ "pm003Count": -1.0 }));

        assert_eq!(snapshot.pm03_count, None);
    }

    #[test]
    fn treats_negative_particulate_numeric_strings_as_missing() {
        let snapshot = parse_snapshot(&json!({
            "pm02": "-1.0",
            "pm01": "-2.0",
            "pm10": "-3.0",
            "pm003Count": "-4.0"
        }));

        assert_eq!(snapshot.pm25, None);
        assert_eq!(snapshot.aqi, None);
        assert_eq!(snapshot.pm1, None);
        assert_eq!(snapshot.pm10, None);
        assert_eq!(snapshot.pm03_count, None);
    }

    #[test]
    fn keeps_zero_particulate_values_valid() {
        let snapshot = parse_snapshot(&json!({
            "pm02": 0.0,
            "pm01": 0.0,
            "pm10": 0.0,
            "pm003Count": 0.0
        }));

        assert_eq!(snapshot.pm25, Some(0.0));
        assert_eq!(snapshot.aqi, Some(0.0));
        assert_eq!(snapshot.pm1, Some(0.0));
        assert_eq!(snapshot.pm10, Some(0.0));
        assert_eq!(snapshot.pm03_count, Some(0.0));
    }

    #[test]
    fn ignores_non_finite_numeric_strings() {
        let snapshot = parse_snapshot(&json!({
            "pm02": "NaN",
            "pm01": "inf",
            "pm10": "-inf",
            "pm003Count": "Infinity",
            "aqi": "NaN",
            "co2": "inf"
        }));

        assert_eq!(snapshot.pm25, None);
        assert_eq!(snapshot.aqi, None);
        assert_eq!(snapshot.pm1, None);
        assert_eq!(snapshot.pm10, None);
        assert_eq!(snapshot.pm03_count, None);
        assert_eq!(snapshot.co2, None);
    }

    #[test]
    fn treats_negative_explicit_aqi_as_missing_and_uses_pm25_fallback() {
        let snapshot = parse_snapshot(&json!({ "aqi": -1.0, "pm02": 12.1 }));

        assert_eq!(snapshot.aqi, Some(57.0));
    }

    #[test]
    fn treats_out_of_range_explicit_aqi_as_missing_and_uses_pm25_fallback() {
        let snapshot = parse_snapshot(&json!({ "aqi": 501.0, "pm02": 12.1 }));

        assert_eq!(snapshot.aqi, Some(57.0));
    }

    #[test]
    fn treats_negative_co2_as_missing() {
        let snapshot = parse_snapshot(&json!({ "co2": -1.0 }));

        assert_eq!(snapshot.co2, None);
    }

    #[test]
    fn treats_out_of_range_humidity_as_missing() {
        let low_snapshot = parse_snapshot(&json!({ "rhum": -0.1 }));
        let high_snapshot = parse_snapshot(&json!({ "rhum": 100.1 }));

        assert_eq!(low_snapshot.humidity, None);
        assert_eq!(high_snapshot.humidity, None);
    }

    #[test]
    fn treats_negative_tvoc_as_missing() {
        let snapshot = parse_snapshot(&json!({ "tvocIndex": -1.0 }));

        assert_eq!(snapshot.tvoc, None);
    }

    #[test]
    fn treats_negative_nox_as_missing() {
        let snapshot = parse_snapshot(&json!({ "noxIndex": -1.0 }));

        assert_eq!(snapshot.nox, None);
    }

    #[test]
    fn prefers_explicit_aqi_over_pm25_fallback() {
        let snapshot = parse_snapshot(&json!({ "aqi": 42, "pm02": 35.4 }));

        assert_eq!(snapshot.aqi, Some(42.0));
    }

    #[test]
    fn prefers_compensated_temperature_and_humidity() {
        let snapshot = parse_snapshot(&json!({
            "atmp": 19.0,
            "atmpCompensated": 20.5,
            "rhum": 39.0,
            "rhumCompensated": 44.0
        }));

        assert_eq!(snapshot.temperature_c, Some(20.5));
        assert_eq!(snapshot.humidity, Some(44.0));
    }
}
