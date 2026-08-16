//! Replacing a file's contents in one step.
//!
//! Shared by the two things this tool writes: the config file it shares with
//! the AirGradient desktop app, and the reading history behind the dashboard's
//! chart. Both are read by something else — the desktop app, or the next run of
//! this one — so neither can afford to be caught half-written.

use std::{fs, io, path::Path, process};

/// Replaces `path` with `contents` in a single step.
///
/// Writing straight to the destination would truncate it first and only then
/// start writing, so a crash, a full disk, or a killed process in between
/// leaves a half-written or empty file. For the config that takes down both
/// this CLI and the desktop app that shares it; for the history it costs every
/// reading ever recorded.
///
/// Instead the new content goes to a sibling temporary file, is flushed to disk
/// with `sync_all`, and is then moved into place with `rename`, which is atomic
/// on the same filesystem: a reader sees either the old file or the new one,
/// never a partial one. The temporary file is removed if any step fails.
///
/// Note this makes each write atomic; it does not serialize two processes
/// writing at once. Guarding a read-modify-write against a concurrent writer is
/// a separate problem — see `config::store::update_config`.
pub(crate) fn write_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    let temp_path = temp_path_for(path);

    let write_result = (|| {
        let mut file = fs::File::create(&temp_path)?;
        io::Write::write_all(&mut file, contents)?;
        file.sync_all()?;
        fs::rename(&temp_path, path)
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    write_result
}

/// A sibling path for the temporary file, in the same directory so the later
/// `rename` stays on one filesystem. The process id keeps two CLI processes
/// writing at the same time from clobbering each other's temporary file.
fn temp_path_for(path: &Path) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_contents_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("thing.json");
        fs::write(&path, b"old").unwrap();

        write_atomically(&path, b"new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name != "thing.json")
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn a_failed_write_leaves_the_original_alone() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where the file should be: `File::create` fails, and the
        // rename never happens.
        let path = dir.path().join("occupied");
        fs::create_dir(&path).unwrap();

        assert!(write_atomically(&path, b"new").is_err());
        assert!(path.is_dir());
    }
}
