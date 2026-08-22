//! Reading and writing the config file.
//!
//! Every write goes through [`write_config`], which merges into the existing
//! JSON object instead of overwriting it — the desktop app stores fields this
//! CLI does not model, and a plain serialize-and-replace would silently drop
//! them. The file is replaced atomically ([`write_atomically`]) so an
//! interrupted write cannot leave the shared file truncated.

use std::{fs, path::Path};

use serde_json::{Map, Value};

use crate::{atomic_file, device_url::DeviceUrl};

use super::{
    Config, ConfigError, DisplayConfig, RefreshInterval,
    lossy::{config_from_object_lossy, display_config_from_object},
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
///
/// This is the blind write: it does not care what the file said a moment ago.
/// Callers that read, changed one field, and are writing the result back go
/// through [`update_config`] instead, which checks that nothing else moved in
/// between.
pub fn write_config(path: &Path, config: &Config) -> Result<(), ConfigError> {
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

    // Errors are given the config's own shape here rather than in the shared
    // helper, which has no opinion about what file it is writing.
    atomic_file::write_atomically(path, contents.as_bytes()).map_err(|source| ConfigError::Write {
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
pub fn set_url(path: &Path, url: &str) -> Result<ConfigUpdate, ConfigError> {
    let normalized_url = DeviceUrl::parse(url)?;
    update_config(path, |config| {
        config.server_url = Some(normalized_url.as_str().to_owned());
    })
}

pub fn set_refresh_interval(path: &Path, seconds: u64) -> Result<ConfigUpdate, ConfigError> {
    let refresh_interval = RefreshInterval::from_secs(seconds)?;
    update_config(path, |config| config.refresh_interval = refresh_interval)
}

/// Stores a theme id without validating it. Ids are not validated here on
/// purpose: an unknown id falls back to the default theme at startup, so a
/// theme added by a newer build is stored, not rejected.
pub fn set_theme(path: &Path, id: &str) -> Result<ConfigUpdate, ConfigError> {
    update_config(path, |config| config.theme = id.to_owned())
}

/// Stores the settings a running TUI session can change, in one write.
///
/// Used by the palette's `:save`, which persists what the session currently
/// looks like. `url` is passed already normalized, because the session only
/// ever holds a URL that came through [`DeviceUrl`]. Fields this CLI does not
/// model, and the two the TUI never touches, are left as the file has them.
///
/// A session with no URL leaves the stored one alone rather than writing null
/// over it. `:save` means "keep what I have", and a TUI started without a URL
/// has nothing to say about that field — but it can be open for hours, so
/// writing the absence would erase a URL the desktop app set in the meantime.
/// That is a different thing from clearing the URL, which the config editor's
/// empty-field path still does explicitly.
pub fn set_session(
    path: &Path,
    url: Option<&str>,
    refresh_interval: RefreshInterval,
    theme: &str,
) -> Result<ConfigUpdate, ConfigError> {
    update_config(path, |config| {
        if let Some(url) = url {
            config.server_url = Some(url.to_owned());
        }
        config.refresh_interval = refresh_interval;
        config.theme = theme.to_owned();
    })
}

/// Read-modify-write for a single field.
///
/// The read is tolerant ([`config_from_object_lossy`]) so that setting one
/// field still works when another is malformed — otherwise a bad value in the
/// file would block the very command that repairs it.
///
/// # Concurrent writers
///
/// The file is shared with the AirGradient desktop app, so between the read
/// above and the write below somebody else may have written it. Blindly
/// writing anyway is a lost update: their change is gone, silently, and the
/// user's only clue is a setting that reverted on its own.
///
/// So the file is checked before the write and the write is refused if it
/// moved. This is not a true compare-and-set — `rename` cannot be made
/// conditional portably — and a writer that lands inside the microseconds
/// between [`fingerprint`] and the rename still wins. What it does do is
/// shrink the window from "the whole read-modify-write, including a user
/// typing a URL into the config editor" to that gap, and turn what is left
/// from silent data loss into an error the user can act on by re-running the
/// command.
pub(super) fn update_config(
    path: &Path,
    apply: impl FnOnce(&mut Config),
) -> Result<ConfigUpdate, ConfigError> {
    let before = config_version(path);

    let object = read_config_object(path)?;
    let (mut config, warnings) = config_from_object_lossy(&object);
    apply(&mut config);

    write_config_if_unchanged(path, &config, &before)?;
    Ok(ConfigUpdate { config, warnings })
}

/// The result of writing one field: the config as it now stands on disk, and
/// anything else that had to be repaired to get there.
///
/// The two travel together because the repairs are a side effect of the write
/// -- a caller that takes the config without looking at the warnings is
/// rewriting fields the user did not ask it to touch, silently, which is what
/// [`config_from_object_lossy`] exists to make visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigUpdate {
    pub config: Config,
    /// What else was repaired on the way, in the same words `config show`
    /// uses. Empty when the file was already well-formed.
    pub warnings: Vec<String>,
}

/// What the file looked like when it was read, for detecting that somebody
/// else has written it since.
///
/// Opaque on purpose: callers carry one from their read to their write and
/// compare it only by handing it back, so there is nothing to interpret and
/// no way to construct a version that did not come from a real observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigVersion(Option<(u64, Option<std::time::SystemTime>)>);

/// Fingerprints the file as it is now. Take this *before* reading, so the
/// window being guarded includes the read itself.
pub fn config_version(path: &Path) -> ConfigVersion {
    ConfigVersion(fingerprint(path))
}

/// Writes `config`, unless the file has changed since `expected` was taken.
///
/// The guard every read-modify-write against this file needs, expressed once.
/// See [`update_config`] for what it does and does not cover: a writer landing
/// inside the microseconds between the check and the rename still wins, but a
/// change made at any point during the caller's read-modify-write — including
/// a user sitting in the config editor — is caught and reported instead of
/// silently overwriting somebody else's edit.
pub fn write_config_if_unchanged(
    path: &Path,
    config: &Config,
    expected: &ConfigVersion,
) -> Result<(), ConfigError> {
    if config_version(path) != *expected {
        return Err(ConfigError::ConcurrentModification {
            path: path.to_path_buf(),
        });
    }

    write_config(path, config)
}

/// What the file looked like, for detecting that somebody else wrote it.
///
/// Size and modification time, which is what is cheaply available on every
/// platform this ships to. A concurrent write that leaves both unchanged —
/// same byte count, within the filesystem's timestamp granularity — is not
/// detected; a real edit by the desktop app changes at least one. A missing or
/// unreadable file fingerprints as `None`, so a file appearing between the two
/// calls is caught too.
fn fingerprint(path: &Path) -> Option<(u64, Option<std::time::SystemTime>)> {
    let metadata = fs::metadata(path).ok()?;

    Some((metadata.len(), metadata.modified().ok()))
}
