//! Reading and writing the config file.
//!
//! Every write goes through [`write_config`], which merges into the existing
//! JSON object instead of overwriting it — the desktop app stores fields this
//! CLI does not model, and a plain serialize-and-replace would silently drop
//! them.

use std::{fs, path::Path};

use serde_json::{Map, Value};

use crate::device;

use super::{
    Config, ConfigError, DisplayConfig,
    lossy::{config_from_object_lossy, display_config_from_object},
    validate_refresh_interval,
};

/// Reads the config strictly: any malformed field is an error.
///
/// Use [`read_display_config`] instead when the caller should show a broken
/// config rather than refuse to run.
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

/// Reads the config tolerantly: unparseable fields fall back to defaults and
/// are reported as warnings rather than failing the read.
pub fn read_display_config(path: &Path) -> Result<DisplayConfig, ConfigError> {
    let object = read_config_object(path)?;
    Ok(display_config_from_object(&object))
}

/// Normalizes an already-loaded config's URL for display, reporting rather
/// than discarding a URL that cannot be normalized — the stored value is kept
/// so the user can see what needs fixing.
pub fn normalized_display_config(mut config: Config) -> DisplayConfig {
    let Some(server_url) = config.server_url.as_deref() else {
        return DisplayConfig {
            config,
            warnings: Vec::new(),
        };
    };

    if server_url.trim().is_empty() {
        return DisplayConfig {
            config,
            warnings: Vec::new(),
        };
    }

    match device::normalize_base_url(server_url) {
        Ok(normalized) => {
            config.server_url = Some(normalized.to_string());
            DisplayConfig {
                config,
                warnings: Vec::new(),
            }
        }
        Err(error) => {
            let warning =
                format!("stored server_url `{server_url}` could not be normalized: {error}");
            DisplayConfig {
                config,
                warnings: vec![warning],
            }
        }
    }
}

/// Writes `config`, preserving every key already in the file that this CLI
/// does not model. Creates the parent directory if it does not exist.
pub fn write_config(path: &Path, config: &Config) -> Result<(), ConfigError> {
    validate_refresh_interval(config.refresh_interval_secs)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut object = read_config_object(path)?;
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

/// The config file as a raw JSON object, or an empty one if it does not exist
/// yet. A non-object top level is rejected outright: merging a write into it
/// is not defined.
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

/// Normalizes `url` through the device boundary before storing it, so the
/// file only ever holds URLs the fetch path can actually use.
pub fn set_url(path: &Path, url: &str) -> Result<Config, ConfigError> {
    let normalized_url = device::normalize_base_url(url)?;
    update_config(path, |config| {
        config.server_url = Some(normalized_url.as_str().to_owned());
    })
}

pub fn set_refresh_interval(path: &Path, seconds: u64) -> Result<Config, ConfigError> {
    validate_refresh_interval(seconds)?;
    update_config(path, |config| config.refresh_interval_secs = seconds)
}

/// Stores a theme id without validating it. Ids are not validated here on
/// purpose: an unknown id falls back to the default theme at startup, so a
/// theme added by a newer build is stored, not rejected.
pub fn set_theme(path: &Path, id: &str) -> Result<Config, ConfigError> {
    update_config(path, |config| config.theme = id.to_owned())
}

/// Read-modify-write for a single field.
///
/// The read is tolerant ([`config_from_object_lossy`]) so that setting one
/// field still works when another is malformed — otherwise a bad value in the
/// file would block the very command that repairs it.
fn update_config(path: &Path, apply: impl FnOnce(&mut Config)) -> Result<Config, ConfigError> {
    let object = read_config_object(path)?;
    let mut config = config_from_object_lossy(&object);
    apply(&mut config);
    write_config(path, &config)?;
    Ok(config)
}
