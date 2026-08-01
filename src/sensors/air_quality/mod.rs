//! Parsing a device measurement payload into a normalized snapshot.
//!
//! AirGradient firmware revisions and the local vs. cloud endpoints spell the
//! same reading differently, so every field is looked up through a list of
//! accepted key spellings and bounded to a physically plausible range. An
//! out-of-range or unparseable value becomes `None` rather than failing the
//! whole payload — one bad sensor must not blank the entire dashboard.

#[cfg(test)]
mod tests;

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

// Parser domain limits reject impossible transport/glitch values before they reach
// shared presentation. The caps intentionally sit above normal threshold ranges.
const MAX_AQI: f64 = 500.0;
const MAX_CO2_PPM: f64 = 40_000.0;
const MAX_TVOC_INDEX: f64 = 500.0;
const MAX_NOX_INDEX: f64 = 500.0;
const MAX_PM_MASS_UG_M3: f64 = 1_000.0;
const MAX_PM03_COUNT_PER_DL: f64 = 1_000_000.0;
const MIN_TEMPERATURE_C: f64 = -40.0;
const MAX_TEMPERATURE_C: f64 = 85.0;
const MIN_HUMIDITY_PERCENT: f64 = 0.0;
const MAX_HUMIDITY_PERCENT: f64 = 100.0;

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
        let pm25 = find_bounded_number(payload, PM25_KEYS, 0.0, MAX_PM_MASS_UG_M3);
        let explicit_aqi = find_bounded_number(payload, AQI_KEYS, 0.0, MAX_AQI);

        Self {
            aqi: explicit_aqi.or_else(|| pm25.map(us_aqi_from_pm25)),
            co2: find_bounded_number(payload, CO2_KEYS, 0.0, MAX_CO2_PPM),
            pm25,
            pm1: find_bounded_number(payload, PM1_KEYS, 0.0, MAX_PM_MASS_UG_M3),
            pm10: find_bounded_number(payload, PM10_KEYS, 0.0, MAX_PM_MASS_UG_M3),
            pm03_count: find_bounded_number(payload, PM03_COUNT_KEYS, 0.0, MAX_PM03_COUNT_PER_DL),
            tvoc: find_bounded_number(payload, TVOC_KEYS, 0.0, MAX_TVOC_INDEX),
            nox: find_bounded_number(payload, NOX_KEYS, 0.0, MAX_NOX_INDEX),
            temperature_c: find_bounded_number(
                payload,
                TEMPERATURE_COMPENSATED_KEYS,
                MIN_TEMPERATURE_C,
                MAX_TEMPERATURE_C,
            )
            .or_else(|| {
                find_bounded_number(
                    payload,
                    TEMPERATURE_KEYS,
                    MIN_TEMPERATURE_C,
                    MAX_TEMPERATURE_C,
                )
            }),
            humidity: find_bounded_number(
                payload,
                HUMIDITY_COMPENSATED_KEYS,
                MIN_HUMIDITY_PERCENT,
                MAX_HUMIDITY_PERCENT,
            )
            .or_else(|| {
                find_bounded_number(
                    payload,
                    HUMIDITY_KEYS,
                    MIN_HUMIDITY_PERCENT,
                    MAX_HUMIDITY_PERCENT,
                )
            }),
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

fn find_bounded_number(value: &Value, keys: &[&str], min: f64, max: f64) -> Option<f64> {
    match value {
        Value::Object(map) => {
            for wanted in keys {
                if let Some(found) = map
                    .iter()
                    .filter(|(key, _)| key_matches(key, wanted))
                    .filter_map(|(_, value)| value_as_number(value))
                    .find(|value| (min..=max).contains(value))
                {
                    return Some(found);
                }
            }

            map.values().find_map(|child| {
                matches!(child, Value::Object(_) | Value::Array(_))
                    .then(|| find_bounded_number(child, keys, min, max))
                    .flatten()
            })
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_bounded_number(child, keys, min, max)),
        _ => None,
    }
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
    actual.eq_ignore_ascii_case(expected)
        || (normalized_key(actual) == normalized_key(expected)
            && numeric_runs(actual) == numeric_runs(expected))
}

fn normalized_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn numeric_runs(key: &str) -> Vec<String> {
    let mut runs = Vec::new();
    let mut current = String::new();

    for character in key.chars() {
        if character.is_ascii_digit() {
            current.push(character);
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        runs.push(current);
    }

    runs
}
