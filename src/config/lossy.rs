//! Tolerant parsing of a config object.
//!
//! Every field falls back to its default instead of failing the whole read,
//! and records why. `serde` cannot express this: one wrong type there aborts
//! the deserialization, which would leave a user with a hand-edited config
//! unable to run the CLI that fixes it.

use serde_json::{Map, Value};

use crate::device;

use super::{
    Config, DEFAULT_REFRESH_INTERVAL_SECS, DisplayConfig, MAX_REFRESH_INTERVAL_SECS,
    MIN_REFRESH_INTERVAL_SECS, default_notifications_enabled, default_theme,
    validate_refresh_interval,
};

/// Parses for display: same fallbacks as [`config_from_object_lossy`], but
/// the server URL is also normalized, and every repair is reported.
pub(super) fn display_config_from_object(object: &Map<String, Value>) -> DisplayConfig {
    let mut warnings = Vec::new();
    let mut config = parse_config(object, &mut warnings);
    config.server_url = normalize_parsed_url(config.server_url, &mut warnings);

    DisplayConfig { config, warnings }
}

/// Parses for a read-modify-write, discarding the warnings.
///
/// The URL is deliberately left as stored: rewriting the file must not
/// silently rewrite a field the user did not ask to change.
pub(super) fn config_from_object_lossy(object: &Map<String, Value>) -> Config {
    parse_config(object, &mut Vec::new())
}

fn parse_config(object: &Map<String, Value>, warnings: &mut Vec<String>) -> Config {
    Config {
        server_url: parse_raw_server_url(object.get("server_url"), warnings),
        refresh_interval_secs: parse_refresh_interval(
            object.get("refresh_interval_secs"),
            warnings,
        ),
        notifications_enabled: parse_bool_field(
            object.get("notifications_enabled"),
            "notifications_enabled",
            default_notifications_enabled(),
            warnings,
        ),
        start_minimized: parse_bool_field(
            object.get("start_minimized"),
            "start_minimized",
            false,
            warnings,
        ),
        theme: parse_theme(object.get("theme"), warnings),
    }
}

/// Normalizes a stored URL, keeping the original when it cannot be
/// normalized so the user sees the value that needs fixing rather than a
/// blank field. An all-whitespace URL is passed through as-is: it means
/// "unset" and normalizing it would only produce a confusing error.
fn normalize_parsed_url(server_url: Option<String>, warnings: &mut Vec<String>) -> Option<String> {
    let server_url = server_url?;
    if server_url.trim().is_empty() {
        return Some(server_url);
    }

    match device::normalize_base_url(&server_url) {
        Ok(normalized) => Some(normalized.to_string()),
        Err(error) => {
            warnings.push(format!(
                "stored server_url `{server_url}` could not be normalized: {error}"
            ));
            Some(server_url)
        }
    }
}

fn parse_theme(value: Option<&Value>, warnings: &mut Vec<String>) -> String {
    match value {
        None | Some(Value::Null) => default_theme(),
        Some(Value::String(theme)) => theme.clone(),
        Some(value) => {
            warnings.push(format!(
                "invalid theme: expected string, got {}; using {}",
                value_type(value),
                default_theme()
            ));
            default_theme()
        }
    }
}

fn parse_raw_server_url(value: Option<&Value>, warnings: &mut Vec<String>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(url)) => Some(url.clone()),
        Some(value) => {
            warnings.push(format!(
                "invalid server_url: expected string or null, got {}; using null",
                value_type(value)
            ));
            None
        }
    }
}

/// An out-of-range interval is treated exactly like a wrong type: the stored
/// number is not clamped, because a silently clamped value would look like
/// the config was accepted as written.
fn parse_refresh_interval(value: Option<&Value>, warnings: &mut Vec<String>) -> u64 {
    match value {
        None => DEFAULT_REFRESH_INTERVAL_SECS,
        Some(Value::Number(number)) => {
            if let Some(seconds) = number.as_u64()
                && validate_refresh_interval(seconds).is_ok()
            {
                return seconds;
            }

            warnings.push(format!(
                "invalid refresh_interval_secs: expected integer from {MIN_REFRESH_INTERVAL_SECS} to {MAX_REFRESH_INTERVAL_SECS}, got {number}; using {DEFAULT_REFRESH_INTERVAL_SECS}"
            ));
            DEFAULT_REFRESH_INTERVAL_SECS
        }
        Some(value) => {
            warnings.push(format!(
                "invalid refresh_interval_secs: expected integer from {MIN_REFRESH_INTERVAL_SECS} to {MAX_REFRESH_INTERVAL_SECS}, got {}; using {DEFAULT_REFRESH_INTERVAL_SECS}",
                value_type(value)
            ));
            DEFAULT_REFRESH_INTERVAL_SECS
        }
    }
}

fn parse_bool_field(
    value: Option<&Value>,
    field: &str,
    default: bool,
    warnings: &mut Vec<String>,
) -> bool {
    match value {
        None => default,
        Some(Value::Bool(value)) => *value,
        Some(value) => {
            warnings.push(format!(
                "invalid {field}: expected boolean, got {}; using {default}",
                value_type(value)
            ));
            default
        }
    }
}

/// The JSON type name, for warnings that tell the user what they wrote
/// instead of only what was expected.
fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
