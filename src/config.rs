use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: None,
            refresh_interval_secs: DEFAULT_REFRESH_INTERVAL_SECS,
            notifications_enabled: true,
            start_minimized: false,
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

pub fn normalized_display_config(mut config: Config) -> Result<Config, ConfigError> {
    if let Some(server_url) = config.server_url.as_deref() {
        config.server_url = Some(crate::device::normalize_base_url(server_url)?.to_string());
    }

    Ok(config)
}

pub fn write_config(path: &Path, config: &Config) -> Result<(), ConfigError> {
    validate_refresh_interval(config.refresh_interval_secs)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    // Writes intentionally emit only the known desktop-compatible schema.
    // Unknown sibling fields from future desktop versions are not preserved yet.
    let mut contents = serde_json::to_string_pretty(config).map_err(ConfigError::Serialize)?;
    contents.push('\n');

    fs::write(path, contents).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}

pub fn set_url(path: &Path, url: &str) -> Result<Config, ConfigError> {
    let normalized_url = crate::device::normalize_base_url(url)?;
    set_normalized_url(path, normalized_url.as_str())
}

fn set_normalized_url(path: &Path, normalized_url: &str) -> Result<Config, ConfigError> {
    let mut config = read_config(path)?;
    config.server_url = Some(normalized_url.to_owned());
    write_config(path, &config)?;
    Ok(config)
}

pub fn set_refresh_interval(path: &Path, seconds: u64) -> Result<Config, ConfigError> {
    validate_refresh_interval(seconds)?;

    let mut config = read_config(path)?;
    config.refresh_interval_secs = seconds;
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
        };

        write_config(&path, &config).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(json["server_url"], "http://192.168.1.201");
        assert_eq!(json["refresh_interval_secs"], 60);
        assert_eq!(json["notifications_enabled"], false);
        assert_eq!(json["start_minimized"], true);
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
}
