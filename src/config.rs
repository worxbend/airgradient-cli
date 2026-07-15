use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

pub const DEFAULT_REFRESH_INTERVAL_SECS: u64 = 30;
pub const MIN_REFRESH_INTERVAL_SECS: u64 = 5;
pub const MAX_REFRESH_INTERVAL_SECS: u64 = 3600;

const CONFIG_DIR_NAME: &str = "airgradient-desktop";
const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server_url: Option<String>,
    #[serde(default = "default_refresh_interval_secs")]
    pub refresh_interval_secs: u64,
    #[serde(default = "default_notifications_enabled")]
    pub notifications_enabled: bool,
    #[serde(default)]
    pub start_minimized: bool,
    /// Built-in theme id (see `airgradient-cli themes`), e.g. `"nord"`. Any
    /// string is accepted here; an unrecognized id resolves to the default
    /// theme when the TUI starts rather than failing to load the config.
    #[serde(default = "default_theme")]
    pub theme: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayConfig {
    pub config: Config,
    pub warnings: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: None,
            refresh_interval_secs: DEFAULT_REFRESH_INTERVAL_SECS,
            notifications_enabled: true,
            start_minimized: false,
            theme: default_theme(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(
        "unable to resolve a default config directory because neither XDG_CONFIG_HOME nor HOME is set"
    )]
    ConfigDirectoryUnavailable,
    #[error(
        "refresh interval must be between {MIN_REFRESH_INTERVAL_SECS} and {MAX_REFRESH_INTERVAL_SECS} seconds, got {0}"
    )]
    RefreshIntervalOutOfRange(u64),
    #[error("failed to read config from {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config from {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("config at {path} must be a JSON object")]
    TopLevelNotObject { path: PathBuf },
    #[error("failed to create config directory {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize config")]
    Serialize(#[source] serde_json::Error),
    #[error("failed to write config to {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to normalize server URL")]
    UrlNormalization(#[from] crate::device::DeviceError),
}

pub fn resolve_config_path(config_path: Option<&Path>) -> Result<PathBuf, ConfigError> {
    let xdg_config_home = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty());
    let home = env::var_os("HOME").filter(|value| !value.is_empty());

    resolve_config_path_from(
        config_path,
        xdg_config_home.as_deref().map(Path::new),
        home.as_deref().map(Path::new),
    )
}

pub fn read_config(path: &Path) -> Result<Config, ConfigError> {
    if !path.exists() {
        return Ok(Config::default());
    }

    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let config =
        serde_json::from_str::<Config>(&contents).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;

    validate_refresh_interval(config.refresh_interval_secs)?;

    Ok(config)
}

pub fn normalized_display_config(mut config: Config) -> DisplayConfig {
    let warning = if let Some(server_url) = config.server_url.as_deref() {
        if server_url.trim().is_empty() {
            None
        } else {
            match crate::device::normalize_base_url(server_url) {
                Ok(normalized) => {
                    config.server_url = Some(normalized.to_string());
                    None
                }
                Err(error) => Some(format!(
                    "stored server_url `{server_url}` could not be normalized: {error}"
                )),
            }
        }
    } else {
        None
    };

    DisplayConfig {
        config,
        warnings: warning.into_iter().collect(),
    }
}

pub fn read_display_config(path: &Path) -> Result<DisplayConfig, ConfigError> {
    let object = read_config_object(path)?;
    Ok(display_config_from_object(&object))
}

pub fn write_config(path: &Path, config: &Config) -> Result<(), ConfigError> {
    validate_refresh_interval(config.refresh_interval_secs)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut object = existing_config_object(path)?;
    let known_values = serde_json::to_value(config).map_err(ConfigError::Serialize)?;
    let known_object = known_values
        .as_object()
        .expect("Config serializes to a JSON object");

    for (key, value) in known_object {
        object.insert(key.clone(), value.clone());
    }

    let mut contents =
        serde_json::to_string_pretty(&Value::Object(object)).map_err(ConfigError::Serialize)?;
    contents.push('\n');

    fs::write(path, contents).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn existing_config_object(path: &Path) -> Result<Map<String, Value>, ConfigError> {
    read_config_object(path)
}

fn read_config_object(path: &Path) -> Result<Map<String, Value>, ConfigError> {
    if !path.exists() {
        return Ok(Map::new());
    }

    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let value = serde_json::from_str::<Value>(&contents).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })?;

    match value {
        Value::Object(object) => Ok(object),
        _ => Err(ConfigError::TopLevelNotObject {
            path: path.to_path_buf(),
        }),
    }
}

pub fn set_url(path: &Path, url: &str) -> Result<Config, ConfigError> {
    let normalized_url = crate::device::normalize_base_url(url)?;
    set_normalized_url(path, normalized_url.as_str())
}

fn set_normalized_url(path: &Path, normalized_url: &str) -> Result<Config, ConfigError> {
    let object = read_config_object(path)?;
    let mut config = config_from_object_lossy(&object);
    config.server_url = Some(normalized_url.to_owned());
    write_config(path, &config)?;
    Ok(config)
}

pub fn set_refresh_interval(path: &Path, seconds: u64) -> Result<Config, ConfigError> {
    validate_refresh_interval(seconds)?;

    let object = read_config_object(path)?;
    let mut config = config_from_object_lossy(&object);
    config.refresh_interval_secs = seconds;
    write_config(path, &config)?;
    Ok(config)
}

pub fn set_theme(path: &Path, id: &str) -> Result<Config, ConfigError> {
    let object = read_config_object(path)?;
    let mut config = config_from_object_lossy(&object);
    config.theme = id.to_owned();
    write_config(path, &config)?;
    Ok(config)
}

pub fn validate_refresh_interval(seconds: u64) -> Result<(), ConfigError> {
    if (MIN_REFRESH_INTERVAL_SECS..=MAX_REFRESH_INTERVAL_SECS).contains(&seconds) {
        Ok(())
    } else {
        Err(ConfigError::RefreshIntervalOutOfRange(seconds))
    }
}

fn resolve_config_path_from(
    config_path: Option<&Path>,
    xdg_config_home: Option<&Path>,
    home: Option<&Path>,
) -> Result<PathBuf, ConfigError> {
    if let Some(config_path) = config_path {
        return Ok(config_path.to_path_buf());
    }

    if let Some(xdg_config_home) = xdg_config_home {
        return Ok(xdg_config_home.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME));
    }

    if let Some(home) = home {
        return Ok(home
            .join(".config")
            .join(CONFIG_DIR_NAME)
            .join(CONFIG_FILE_NAME));
    }

    Err(ConfigError::ConfigDirectoryUnavailable)
}

fn default_refresh_interval_secs() -> u64 {
    DEFAULT_REFRESH_INTERVAL_SECS
}

fn default_notifications_enabled() -> bool {
    true
}

fn default_theme() -> String {
    "default".to_string()
}

fn display_config_from_object(object: &Map<String, Value>) -> DisplayConfig {
    let mut config = Config::default();
    let mut warnings = Vec::new();

    config.server_url = parse_display_server_url(object.get("server_url"), &mut warnings);
    config.refresh_interval_secs =
        parse_refresh_interval(object.get("refresh_interval_secs"), &mut warnings);
    config.notifications_enabled = parse_bool_field(
        object.get("notifications_enabled"),
        "notifications_enabled",
        default_notifications_enabled(),
        &mut warnings,
    );
    config.start_minimized = parse_bool_field(
        object.get("start_minimized"),
        "start_minimized",
        false,
        &mut warnings,
    );
    config.theme = parse_theme(object.get("theme"), &mut warnings);

    DisplayConfig { config, warnings }
}

fn config_from_object_lossy(object: &Map<String, Value>) -> Config {
    let mut warnings = Vec::new();
    Config {
        server_url: parse_raw_server_url(object.get("server_url"), &mut warnings),
        refresh_interval_secs: parse_refresh_interval(
            object.get("refresh_interval_secs"),
            &mut warnings,
        ),
        notifications_enabled: parse_bool_field(
            object.get("notifications_enabled"),
            "notifications_enabled",
            default_notifications_enabled(),
            &mut warnings,
        ),
        start_minimized: parse_bool_field(
            object.get("start_minimized"),
            "start_minimized",
            false,
            &mut warnings,
        ),
        theme: parse_theme(object.get("theme"), &mut warnings),
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

fn parse_display_server_url(value: Option<&Value>, warnings: &mut Vec<String>) -> Option<String> {
    let server_url = parse_raw_server_url(value, warnings)?;
    if server_url.trim().is_empty() {
        return Some(server_url);
    }

    match crate::device::normalize_base_url(&server_url) {
        Ok(normalized) => Some(normalized.to_string()),
        Err(error) => {
            warnings.push(format!(
                "stored server_url `{server_url}` could not be normalized: {error}"
            ));
            Some(server_url)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values_match_desktop_contract() {
        let config = Config::default();

        assert_eq!(config.server_url, None);
        assert_eq!(config.refresh_interval_secs, 30);
        assert!(config.notifications_enabled);
        assert!(!config.start_minimized);
        assert_eq!(config.theme, "default");

        let from_empty_json: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(from_empty_json, config);
    }

    #[test]
    fn path_resolution_prefers_explicit_override() {
        let override_path = Path::new("/tmp/custom-airgradient.json");
        let resolved = resolve_config_path_from(
            Some(override_path),
            Some(Path::new("/tmp/xdg")),
            Some(Path::new("/tmp/home")),
        )
        .unwrap();

        assert_eq!(resolved, override_path);
    }

    #[test]
    fn path_resolution_uses_xdg_config_home() {
        let resolved = resolve_config_path_from(
            None,
            Some(Path::new("/tmp/xdg")),
            Some(Path::new("/tmp/home")),
        )
        .unwrap();

        assert_eq!(
            resolved,
            PathBuf::from("/tmp/xdg/airgradient-desktop/config.json")
        );
    }

    #[test]
    fn path_resolution_falls_back_to_home_config() {
        let resolved = resolve_config_path_from(None, None, Some(Path::new("/tmp/home"))).unwrap();

        assert_eq!(
            resolved,
            PathBuf::from("/tmp/home/.config/airgradient-desktop/config.json")
        );
    }

    #[test]
    fn refresh_interval_bounds_are_enforced() {
        assert!(validate_refresh_interval(MIN_REFRESH_INTERVAL_SECS).is_ok());
        assert!(validate_refresh_interval(DEFAULT_REFRESH_INTERVAL_SECS).is_ok());
        assert!(validate_refresh_interval(MAX_REFRESH_INTERVAL_SECS).is_ok());

        assert!(matches!(
            validate_refresh_interval(MIN_REFRESH_INTERVAL_SECS - 1),
            Err(ConfigError::RefreshIntervalOutOfRange(4))
        ));
        assert!(matches!(
            validate_refresh_interval(MAX_REFRESH_INTERVAL_SECS + 1),
            Err(ConfigError::RefreshIntervalOutOfRange(3601))
        ));
    }

    #[test]
    fn read_config_rejects_out_of_bounds_refresh_interval() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("config.json");
        fs::write(
            &path,
            r#"{"server_url":null,"refresh_interval_secs":1,"notifications_enabled":true,"start_minimized":false}"#,
        )
        .unwrap();

        assert!(matches!(
            read_config(&path),
            Err(ConfigError::RefreshIntervalOutOfRange(1))
        ));
    }

    #[test]
    fn read_missing_config_returns_defaults() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("missing.json");

        assert_eq!(read_config(&path).unwrap(), Config::default());
    }

    #[test]
    fn read_write_json_round_trip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("nested").join("config.json");
        let config = Config {
            server_url: Some("http://192.168.1.201".to_owned()),
            refresh_interval_secs: 60,
            notifications_enabled: false,
            start_minimized: true,
            theme: "nord".to_owned(),
        };

        write_config(&path, &config).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(json["server_url"], "http://192.168.1.201");
        assert_eq!(json["refresh_interval_secs"], 60);
        assert_eq!(json["notifications_enabled"], false);
        assert_eq!(json["start_minimized"], true);
        assert_eq!(json["theme"], "nord");
        assert_eq!(read_config(&path).unwrap(), config);
    }

    #[test]
    fn set_url_normalizes_with_device_boundary_before_persisting() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("config.json");

        let config = set_url(&path, "192.168.1.201/sensors").unwrap();

        assert_eq!(config.server_url.as_deref(), Some("http://192.168.1.201/"));
        assert_eq!(
            read_config(&path).unwrap().server_url.as_deref(),
            Some("http://192.168.1.201/")
        );
    }

    #[test]
    fn set_refresh_interval_persists_valid_value() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("config.json");

        let config = set_refresh_interval(&path, 120).unwrap();

        assert_eq!(config.refresh_interval_secs, 120);
        assert_eq!(read_config(&path).unwrap().refresh_interval_secs, 120);
    }

    #[test]
    fn set_theme_persists_any_id_without_validation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("config.json");

        let config = set_theme(&path, "dracula").unwrap();

        assert_eq!(config.theme, "dracula");
        assert_eq!(read_config(&path).unwrap().theme, "dracula");
    }

    #[test]
    fn set_theme_preserves_other_fields() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("config.json");
        set_refresh_interval(&path, 90).unwrap();

        let config = set_theme(&path, "gruvbox").unwrap();

        assert_eq!(config.theme, "gruvbox");
        assert_eq!(config.refresh_interval_secs, 90);
    }

    #[test]
    fn unknown_theme_value_type_warns_and_falls_back_to_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("config.json");
        fs::write(&path, r#"{"theme": 42}"#).unwrap();

        let display = read_display_config(&path).unwrap();

        assert_eq!(display.config.theme, "default");
        assert!(display.warnings.iter().any(|w| w.contains("theme")));
    }
}
