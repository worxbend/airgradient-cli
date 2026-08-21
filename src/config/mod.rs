//! The on-disk configuration this CLI shares with the AirGradient desktop
//! app.
//!
//! The file is owned by the desktop app as much as by this CLI, which shapes
//! two rules the rest of this module exists to keep:
//!
//! - **Unknown keys survive.** Writes merge into the existing JSON object
//!   rather than replacing it, so desktop-only fields are never dropped.
//! - **A malformed file is not fatal.** [`lossy`] falls back per field and
//!   reports what it ignored, so one bad value cannot lock a user out of the
//!   CLI that would let them fix it.
//!
//! Reading and writing live in [`store`]; this module owns the types, the
//! bounds, and where the file is.

mod lossy;
mod store;

#[cfg(test)]
mod tests;

use std::{
    env,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use store::{
    read_config, read_display_config, set_refresh_interval, set_session, set_theme, set_url,
    write_config,
};

#[cfg(test)]
use store::update_config;

use crate::device_url::{DeviceUrl, DeviceUrlError};
pub use crate::refresh_interval::{
    DEFAULT_SECS as DEFAULT_REFRESH_INTERVAL_SECS, MAX_SECS as MAX_REFRESH_INTERVAL_SECS,
    MIN_SECS as MIN_REFRESH_INTERVAL_SECS, RefreshInterval,
};

/// Shared with the desktop app — this is its directory, not a CLI-specific
/// one, which is what lets both read and write the same settings.
const CONFIG_DIR_NAME: &str = "airgradient-desktop";
const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server_url: Option<String>,
    #[serde(default, rename = "refresh_interval_secs")]
    pub refresh_interval: RefreshInterval,
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

impl Config {
    /// The stored device address, or `None` when none is configured.
    ///
    /// `server_url` stays a `String` on purpose — the file may hold a value the
    /// domain rejects, and `config show` has to display exactly that so the
    /// user can see what needs fixing. This is the interpretation of that
    /// string, for the callers that need an address rather than a field.
    pub fn device_url(&self) -> Result<Option<DeviceUrl>, DeviceUrlError> {
        DeviceUrl::parse_optional(self.server_url.as_deref())
    }
}

/// A config as it should be shown to the user: values repaired where they
/// could be, plus the list of what had to be repaired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayConfig {
    pub config: Config,
    pub warnings: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: None,
            refresh_interval: RefreshInterval::DEFAULT,
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
    #[error(transparent)]
    RefreshIntervalOutOfRange(#[from] crate::refresh_interval::OutOfRange),
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
    #[error(
        "config at {path} was changed by another program while this command was running; nothing was written — re-run the command"
    )]
    ConcurrentModification { path: PathBuf },
    #[error("failed to write config to {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Transparent rather than wrapped: `DeviceUrlError`'s messages already
    /// name the field and say what to do ("device URL must not embed
    /// credentials; remove the `user:password@` part before the host"), so a
    /// "failed to normalize server URL" wrapper around them only pushed the
    /// useful half behind `-v`.
    #[error(transparent)]
    UrlNormalization(#[from] crate::device_url::DeviceUrlError),
}

/// Where the config file is: an explicit `--config` path if given, otherwise
/// the XDG location, otherwise `~/.config`.
pub fn resolve_config_path(config_path: Option<&Path>) -> Result<PathBuf, ConfigError> {
    // Empty env vars are treated as unset — an exported-but-blank
    // XDG_CONFIG_HOME would otherwise resolve the config to a relative path.
    let xdg_config_home = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty());
    let home = env::var_os("HOME").filter(|value| !value.is_empty());

    resolve_config_path_from(
        config_path,
        xdg_config_home.as_deref().map(Path::new),
        home.as_deref().map(Path::new),
    )
}

/// Path resolution with the environment passed in, so the precedence rules
/// are testable without mutating process-wide env vars.
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

fn default_notifications_enabled() -> bool {
    true
}

fn default_theme() -> String {
    "default".to_string()
}
