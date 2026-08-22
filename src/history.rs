//! The recorded reading history behind the dashboard's chart.
//!
//! Every successful fetch appends one line here, so the chart has a shape to
//! draw the moment the dashboard opens instead of starting blank and filling in
//! over the next quarter of an hour. That is the whole reason this is on disk:
//! a trend is the one thing a live-only dashboard cannot show you.
//!
//! # Shape
//!
//! One JSON object per line ([JSON Lines]), oldest first. That format is chosen
//! for what appending costs: a new reading is a single `write` to the end of
//! the file, with no read of what is already there and nothing to rewrite. A
//! JSON array would mean parsing and re-serializing the whole history every
//! five seconds.
//!
//! It also fails well. A line truncated by a kill or a full disk costs that one
//! reading — [`load`] skips lines it cannot parse and keeps the rest — where a
//! single malformed array takes the lot.
//!
//! # Bounds
//!
//! The file is capped (see [`MAX_STORED_READINGS`]) and trimmed to the most
//! recent readings when it grows past [`TRIM_THRESHOLD_BYTES`]. Checking the
//! size is a `stat`; the trim itself is rare, and rewrites through
//! [`crate::atomic_file`] so an interrupted trim cannot cost the history.
//!
//! [JSON Lines]: https://jsonlines.org/

use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{atomic_file, sensors::SensorSnapshot};

/// How many readings survive a trim.
///
/// At the default 30-second refresh this is about seventeen hours. It is a cap
/// on a local file of a few hundred kilobytes, not a retention policy -- the
/// dashboard only ever replays the newest [`MAX_READING_HISTORY`] of them, and
/// the rest are there for whatever wants to read the file later.
///
/// [`MAX_READING_HISTORY`]: crate::tui::app::MAX_READING_HISTORY
pub const MAX_STORED_READINGS: usize = 2000;

/// File size at which a trim is considered. Comfortably more than
/// [`MAX_STORED_READINGS`] lines, so trimming is occasional rather than
/// something every append pays for.
const TRIM_THRESHOLD_BYTES: u64 = 1024 * 1024;

const HISTORY_DIR_NAME: &str = "airgradient-cli";
const HISTORY_FILE_NAME: &str = "history.jsonl";

/// One recorded reading: what the device reported, and when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    /// Seconds since the Unix epoch. Stored as a bare number rather than a
    /// formatted timestamp so a line stays small and unambiguous — no time
    /// zone, no format to parse differently later.
    pub at_unix_secs: u64,
    pub snapshot: SensorSnapshot,
}

impl Reading {
    pub fn new(at: SystemTime, snapshot: SensorSnapshot) -> Self {
        Self {
            // A clock before the epoch is not worth an error path: it can only
            // come from a badly mis-set system clock, and the chart's x axis
            // treats it as "the oldest thing I have".
            at_unix_secs: at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            snapshot,
        }
    }

    pub fn at(&self) -> SystemTime {
        // A stamp too large for the platform's `SystemTime` reads as "the
        // oldest thing I have", the same rule `new` applies to a pre-epoch
        // clock, so both ends of the range degrade the same way. Adding it
        // unchecked would panic on a line that parsed perfectly well.
        UNIX_EPOCH
            .checked_add(Duration::from_secs(self.at_unix_secs))
            .unwrap_or(UNIX_EPOCH)
    }
}

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("unable to resolve a history directory because neither XDG_DATA_HOME nor HOME is set")]
    DirectoryUnavailable,
    #[error("failed to create history directory {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read reading history from {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to record a reading to {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize a reading")]
    Serialize(#[source] serde_json::Error),
}

/// Where the history file lives: `$XDG_DATA_HOME/airgradient-cli/history.jsonl`,
/// else `~/.local/share/...`.
///
/// Data, not configuration, so this is deliberately not next to `config.json`
/// and deliberately not in the directory shared with the desktop app: nothing
/// else reads it, and a settings file the user might hand-edit should not have
/// thousands of readings appended to it.
pub fn resolve_history_path(explicit: Option<&Path>) -> Result<PathBuf, HistoryError> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }

    // Empty env vars are treated as unset, matching `config::resolve_config_path`.
    let data_home = env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty());
    if let Some(data_home) = data_home {
        return Ok(Path::new(&data_home)
            .join(HISTORY_DIR_NAME)
            .join(HISTORY_FILE_NAME));
    }

    let home = env::var_os("HOME").filter(|value| !value.is_empty());
    if let Some(home) = home {
        return Ok(Path::new(&home)
            .join(".local")
            .join("share")
            .join(HISTORY_DIR_NAME)
            .join(HISTORY_FILE_NAME));
    }

    Err(HistoryError::DirectoryUnavailable)
}

/// Records one reading, creating the file and its directory if needed.
pub fn append(path: &Path, reading: &Reading) -> Result<(), HistoryError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| HistoryError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut line = serde_json::to_string(reading).map_err(HistoryError::Serialize)?;
    line.push('\n');

    let write = |path: &Path| -> std::io::Result<u64> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(line.as_bytes())?;
        file.metadata().map(|metadata| metadata.len())
    };

    let size = write(path).map_err(|source| HistoryError::Write {
        path: path.to_path_buf(),
        source,
    })?;

    if size > TRIM_THRESHOLD_BYTES {
        trim(path)?;
    }

    Ok(())
}

/// Every recorded reading, oldest first.
///
/// Lines that do not parse are skipped rather than failing the load: the last
/// line of an append-only file is the one a crash truncates, and losing the
/// chart because of it would be a poor trade. A missing file is an empty
/// history, not an error — that is simply the first run.
pub fn load(path: &Path) -> Result<Vec<Reading>, HistoryError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path).map_err(|source| HistoryError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let mut readings = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|source| HistoryError::Read {
            path: path.to_path_buf(),
            source,
        })?;

        if let Ok(reading) = serde_json::from_str::<Reading>(&line) {
            readings.push(reading);
        }
    }

    Ok(readings)
}

/// The most recent `limit` readings, oldest first.
pub fn load_recent(path: &Path, limit: usize) -> Result<Vec<Reading>, HistoryError> {
    let mut readings = load(path)?;
    let excess = readings.len().saturating_sub(limit);
    readings.drain(..excess);

    Ok(readings)
}

/// Rewrites the file with only the most recent [`MAX_STORED_READINGS`].
fn trim(path: &Path) -> Result<(), HistoryError> {
    let readings = load_recent(path, MAX_STORED_READINGS)?;

    let mut contents = String::new();
    for reading in &readings {
        contents.push_str(&serde_json::to_string(reading).map_err(HistoryError::Serialize)?);
        contents.push('\n');
    }

    atomic_file::write_atomically(path, contents.as_bytes()).map_err(|source| HistoryError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(aqi: f64) -> SensorSnapshot {
        SensorSnapshot {
            aqi: Some(aqi),
            ..SensorSnapshot::default()
        }
    }

    fn reading(at_secs: u64, aqi: f64) -> Reading {
        Reading::new(UNIX_EPOCH + Duration::from_secs(at_secs), snapshot(aqi))
    }

    #[test]
    fn appends_are_read_back_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");

        append(&path, &reading(10, 40.0)).unwrap();
        append(&path, &reading(20, 45.0)).unwrap();

        let readings = load(&path).unwrap();
        assert_eq!(readings.len(), 2);
        assert_eq!(readings[0].at_unix_secs, 10);
        assert_eq!(readings[1].snapshot.aqi, Some(45.0));
        assert_eq!(readings[0].at(), UNIX_EPOCH + Duration::from_secs(10));
    }

    #[test]
    fn a_missing_file_is_an_empty_history_rather_than_an_error() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(load(&dir.path().join("nothing.jsonl")).unwrap(), Vec::new());
    }

    #[test]
    fn a_truncated_last_line_costs_only_that_reading() {
        // What a kill mid-append leaves behind.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        append(&path, &reading(10, 40.0)).unwrap();
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(br#"{"at_unix_secs":20,"snap"#).unwrap();

        let readings = load(&path).unwrap();

        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].at_unix_secs, 10);
    }

    #[test]
    fn appending_creates_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("nested")
            .join("deeper")
            .join("history.jsonl");

        append(&path, &reading(10, 40.0)).unwrap();

        assert_eq!(load(&path).unwrap().len(), 1);
    }

    #[test]
    fn load_recent_keeps_the_newest_and_drops_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        for index in 0..10 {
            append(&path, &reading(index, f64::from(index as u32))).unwrap();
        }

        let recent = load_recent(&path, 3).unwrap();

        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].at_unix_secs, 7);
        assert_eq!(recent[2].at_unix_secs, 9);

        // Asking for more than there is yields everything.
        assert_eq!(load_recent(&path, 100).unwrap().len(), 10);
    }

    #[test]
    fn trimming_keeps_the_most_recent_readings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        for index in 0..(MAX_STORED_READINGS as u64 + 50) {
            append(&path, &reading(index, 42.0)).unwrap();
        }

        trim(&path).unwrap();
        let readings = load(&path).unwrap();

        assert_eq!(readings.len(), MAX_STORED_READINGS);
        // The oldest 50 went, not the newest.
        assert_eq!(readings[0].at_unix_secs, 50);
        assert_eq!(
            readings[readings.len() - 1].at_unix_secs,
            MAX_STORED_READINGS as u64 + 49
        );
    }

    #[test]
    fn history_path_prefers_xdg_data_home_then_home() {
        let explicit = PathBuf::from("/tmp/explicit.jsonl");
        assert_eq!(
            resolve_history_path(Some(&explicit)).unwrap(),
            explicit,
            "an explicit path wins outright"
        );
    }

    #[test]
    fn an_out_of_range_timestamp_loads_instead_of_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        // A line that parses as JSON but whose stamp overflows `SystemTime`.
        // Reading it must not take down the process that is drawing a chart.
        std::fs::write(
            &path,
            format!("{{\"at_unix_secs\":{},\"snapshot\":{{}}}}\n", u64::MAX),
        )
        .unwrap();

        let readings = load(&path).unwrap();

        assert_eq!(readings.len(), 1);
        assert_eq!(
            readings[0].at(),
            UNIX_EPOCH,
            "clamped to the oldest instant"
        );
    }
}
