#![allow(dead_code, unused_imports)]

#[path = "../src/sensors/mod.rs"]
mod sensors;

use sensors::parse_snapshot;
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let contents = match name {
        "current_airgradient_payload" => include_str!("fixtures/current_airgradient_payload.json"),
        "nested_conflicting_payload" => include_str!("fixtures/nested_conflicting_payload.json"),
        "alternate_field_names_payload" => {
            include_str!("fixtures/alternate_field_names_payload.json")
        }
        "missing_values_payload" => include_str!("fixtures/missing_values_payload.json"),
        "compensated_fallback_payload" => {
            include_str!("fixtures/compensated_fallback_payload.json")
        }
        _ => unreachable!("unknown fixture {name}"),
    };

    serde_json::from_str(contents).expect("fixture should be valid JSON")
}

#[test]
fn parses_current_airgradient_style_fixture() {
    let snapshot = parse_snapshot(&fixture("current_airgradient_payload"));

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
fn top_level_values_win_over_nested_conflicting_values() {
    let snapshot = parse_snapshot(&fixture("nested_conflicting_payload"));

    assert_eq!(snapshot.co2, Some(610.0));
    assert_eq!(snapshot.pm25, Some(8.2));
    assert_eq!(snapshot.temperature_c, Some(22.4));
    assert_eq!(snapshot.humidity, Some(47.5));
    assert_eq!(snapshot.tvoc, Some(301.0));
    assert_eq!(snapshot.nox, Some(45.0));
}

#[test]
fn parses_common_alternate_field_names_and_numeric_strings() {
    let snapshot = parse_snapshot(&fixture("alternate_field_names_payload"));

    assert_eq!(snapshot.aqi, Some(58.0));
    assert_eq!(snapshot.co2, Some(805.0));
    assert_eq!(snapshot.pm25, Some(12.3));
    assert_eq!(snapshot.pm1, Some(5.5));
    assert_eq!(snapshot.pm10, Some(18.6));
    assert_eq!(snapshot.pm03_count, Some(1500.0));
    assert_eq!(snapshot.temperature_c, Some(21.5));
    assert_eq!(snapshot.humidity, Some(44.2));
    assert_eq!(snapshot.tvoc, Some(91.0));
    assert_eq!(snapshot.nox, Some(3.0));
}

#[test]
fn missing_and_invalid_fixture_values_remain_missing() {
    let snapshot = parse_snapshot(&fixture("missing_values_payload"));

    assert_eq!(snapshot.co2, Some(0.0));
    assert_eq!(snapshot.aqi, None);
    assert_eq!(snapshot.pm25, None);
    assert_eq!(snapshot.pm1, None);
    assert_eq!(snapshot.pm10, None);
    assert_eq!(snapshot.pm03_count, None);
    assert_eq!(snapshot.temperature_c, None);
    assert_eq!(snapshot.humidity, None);
    assert_eq!(snapshot.tvoc, None);
    assert_eq!(snapshot.nox, None);
}

#[test]
fn invalid_compensated_temperature_and_humidity_fall_back_to_raw_values() {
    let snapshot = parse_snapshot(&fixture("compensated_fallback_payload"));

    assert_eq!(snapshot.temperature_c, Some(19.8));
    assert_eq!(snapshot.humidity, Some(42.4));
}
