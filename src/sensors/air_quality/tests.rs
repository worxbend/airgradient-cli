//! Tests for tolerant field lookup, bounds, and AQI conversion.

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
fn treats_valid_explicit_aqi_as_precedent_over_invalid_pm25() {
    let snapshot = parse_snapshot(&json!({ "aqi": 42.0, "pm02": 1001.0 }));

    assert_eq!(snapshot.aqi, Some(42.0));
    assert_eq!(snapshot.pm25, None);
}

#[test]
fn ignores_invalid_explicit_aqi_without_invalid_pm25_fallback() {
    let snapshot = parse_snapshot(&json!({ "aqi": 501.0, "pm02": 1001.0 }));

    assert_eq!(snapshot.aqi, None);
    assert_eq!(snapshot.pm25, None);
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
fn treats_absurdly_high_sensor_values_as_missing() {
    let snapshot = parse_snapshot(&json!({
        "co2": 40_000.1,
        "pm02": 1_000.1,
        "pm01": 1_000.1,
        "pm10": 1_000.1,
        "pm003Count": 1_000_000.1,
        "tvocIndex": 500.1,
        "noxIndex": 500.1,
        "atmp": 85.1,
        "rhum": 100.1
    }));

    assert_eq!(snapshot.co2, None);
    assert_eq!(snapshot.pm25, None);
    assert_eq!(snapshot.aqi, None);
    assert_eq!(snapshot.pm1, None);
    assert_eq!(snapshot.pm10, None);
    assert_eq!(snapshot.pm03_count, None);
    assert_eq!(snapshot.tvoc, None);
    assert_eq!(snapshot.nox, None);
    assert_eq!(snapshot.temperature_c, None);
    assert_eq!(snapshot.humidity, None);
}

#[test]
fn treats_absurdly_high_numeric_strings_as_missing() {
    let snapshot = parse_snapshot(&json!({
        "co2": "40000.1",
        "pm02": "1000.1",
        "pm01": "1000.1",
        "pm10": "1000.1",
        "pm003Count": "1000000.1",
        "tvocIndex": "500.1",
        "noxIndex": "500.1",
        "atmp": "85.1",
        "rhum": "100.1",
        "aqi": "500.1"
    }));

    assert_eq!(snapshot.co2, None);
    assert_eq!(snapshot.pm25, None);
    assert_eq!(snapshot.aqi, None);
    assert_eq!(snapshot.pm1, None);
    assert_eq!(snapshot.pm10, None);
    assert_eq!(snapshot.pm03_count, None);
    assert_eq!(snapshot.tvoc, None);
    assert_eq!(snapshot.nox, None);
    assert_eq!(snapshot.temperature_c, None);
    assert_eq!(snapshot.humidity, None);
}

#[test]
fn keeps_sensor_values_at_domain_upper_bounds_valid() {
    let snapshot = parse_snapshot(&json!({
        "co2": 40_000.0,
        "pm02": 1_000.0,
        "pm01": 1_000.0,
        "pm10": 1_000.0,
        "pm003Count": 1_000_000.0,
        "tvocIndex": 500.0,
        "noxIndex": 500.0,
        "atmp": 85.0,
        "rhum": 100.0,
        "aqi": 500.0
    }));

    assert_eq!(snapshot.co2, Some(40_000.0));
    assert_eq!(snapshot.pm25, Some(1_000.0));
    assert_eq!(snapshot.aqi, Some(500.0));
    assert_eq!(snapshot.pm1, Some(1_000.0));
    assert_eq!(snapshot.pm10, Some(1_000.0));
    assert_eq!(snapshot.pm03_count, Some(1_000_000.0));
    assert_eq!(snapshot.tvoc, Some(500.0));
    assert_eq!(snapshot.nox, Some(500.0));
    assert_eq!(snapshot.temperature_c, Some(85.0));
    assert_eq!(snapshot.humidity, Some(100.0));
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
