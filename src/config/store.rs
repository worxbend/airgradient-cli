//! Reading and writing the config file.
//!
//! Every write goes through [`write_config`], which merges into the existing
//! JSON object instead of overwriting it — the desktop app stores fields this
//! CLI does not model, and a plain serialize-and-replace would silently drop
//! them. The file is replaced atomically ([`write_atomically`]) so an
//! interrupted write cannot leave the shared file truncated.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process,
};

use serde_json::{Map, Value};

use crate::device_url::DeviceUrl;

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

    write_atomically(path, contents.as_bytes())
}

/// Replaces `path` with `contents` in a single step.
///
/// Writing straight to the config file would truncate it first and only then
/// start writing, so a crash, a full disk, or a killed process in between
/// leaves a half-written or empty file. That file no longer parses, which takes
/// down not just this CLI but the desktop app that shares it — including the
/// keys this module goes out of its way to preserve.
///
/// Instead the new content goes to a sibling temporary file, is flushed to disk
/// with `sync_all`, and is then moved into place with `rename`, which is atomic
/// on the same filesystem: a reader sees either the old file or the new one,
/// never a partial one. The temporary file is removed if any step fails.
///
/// Note this makes each write atomic; it does not serialize two processes
/// writing at once. If the desktop app writes between this CLI's read and its
/// write, the CLI's version still wins as a whole.
fn write_atomically(path: &Path, contents: &[u8]) -> Result<(), ConfigError> {
    let temp_path = temp_path_for(path);

    let write_result = (|| {
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temp_path, path)
    })();

    write_result.map_err(|source| {
        let _ = fs::remove_file(&temp_path);
        ConfigError::Write {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// A sibling path for the temporary file, in the same directory so the later
/// `rename` stays on one filesystem. The process id keeps two CLI processes
/// writing at the same time from clobbering each other's temporary file.
fn temp_path_for(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", process::id()));
    path.with_file_name(name)
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
    let normalized_url = DeviceUrl::parse(url)?;
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

/// Stores the three settings a running TUI session can change, in one write.
///
/// Used by the palette's `:save`, which persists what the session currently
/// looks like. `url` is passed already normalized, because the session only
/// ever holds a URL that came through the device boundary. Fields this CLI
/// does not model, and the two the TUI never touches, are left as the file
/// has them.
pub fn set_session(
    path: &Path,
    url: Option<&str>,
    refresh_interval_secs: u64,
    theme: &str,
) -> Result<Config, ConfigError> {
    validate_refresh_interval(refresh_interval_secs)?;
    update_config(path, |config| {
        config.server_url = url.map(ToOwned::to_owned);
        config.refresh_interval_secs = refresh_interval_secs;
        config.theme = theme.to_owned();
    })
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
