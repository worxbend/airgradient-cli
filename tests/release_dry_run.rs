use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use tempfile::tempdir;

const BIN_NAME: &str = "airgradient-cli";
const TARGET: &str = "x86_64-unknown-linux-gnu";

#[test]
fn release_dry_run_stages_targeted_archive_and_checksums() {
    let tempdir = tempdir().expect("tempdir should be created");
    let output_dir = tempdir.path().join("dist");
    let binary_path = tempdir.path().join(BIN_NAME);
    write_executable_fixture(&binary_path);

    let output = release_dry_run()
        .args([
            "--skip-build",
            "--binary",
            path_str(&binary_path),
            "--output-dir",
            path_str(&output_dir),
            "--target",
            TARGET,
        ])
        .output()
        .expect("release dry-run script should execute");

    assert_success(&output);

    let version = package_version();
    let artifact_name = format!("{BIN_NAME}-v{version}-{TARGET}.tar.gz");
    let artifact_path = output_dir.join(&artifact_name);
    let checksums_path = output_dir.join("SHA256SUMS");

    assert!(
        artifact_path.is_file(),
        "expected release artifact at {}",
        artifact_path.display()
    );
    assert!(
        checksums_path.is_file(),
        "expected checksum manifest at {}",
        checksums_path.display()
    );

    let archive_entries = archive_entries(&artifact_path);
    assert!(
        archive_entries.iter().any(|entry| entry == BIN_NAME),
        "archive should contain {BIN_NAME}; entries: {archive_entries:?}"
    );
    assert!(
        archive_entries.iter().any(|entry| entry == "LICENSE"),
        "archive should contain LICENSE; entries: {archive_entries:?}"
    );

    let checksum_entries = checksum_entries(&checksums_path);
    assert_eq!(
        checksum_entries,
        vec![artifact_name],
        "SHA256SUMS should cover exactly the staged release artifact"
    );
    for entry in checksum_entries {
        assert!(
            output_dir.join(&entry).is_file(),
            "checksum entry should point to a staged artifact: {entry}"
        );
    }
}

#[test]
fn skip_build_reports_missing_binary_explicitly() {
    let tempdir = tempdir().expect("tempdir should be created");
    let output_dir = tempdir.path().join("dist");
    let missing_binary = tempdir.path().join(BIN_NAME);

    let output = release_dry_run()
        .args([
            "--skip-build",
            "--binary",
            path_str(&missing_binary),
            "--output-dir",
            path_str(&output_dir),
        ])
        .output()
        .expect("release dry-run script should execute");

    assert!(
        !output.status.success(),
        "script should fail for a missing skip-build binary"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("release dry run failed: missing binary for --skip-build"),
        "missing-binary error should be explicit; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains(path_str(&missing_binary)),
        "missing-binary error should include the requested path; stderr:\n{stderr}"
    );
}

fn release_dry_run() -> Command {
    let mut command = Command::new("bash");
    command
        .arg(repo_root().join("scripts/release-dry-run.sh"))
        .current_dir(repo_root());
    command
}

fn write_executable_fixture(path: &Path) {
    fs::write(path, b"#!/usr/bin/env sh\nexit 0\n").expect("fixture binary should be written");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .expect("fixture metadata should be readable")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("fixture binary should be executable");
    }
}

fn package_version() -> String {
    let manifest =
        fs::read_to_string(repo_root().join("Cargo.toml")).expect("Cargo.toml should be readable");
    let mut in_package = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && trimmed.starts_with('[') {
            break;
        }
        if in_package && trimmed.starts_with("version") {
            return trimmed
                .split_once('=')
                .expect("version line should contain '='")
                .1
                .trim()
                .trim_matches('"')
                .to_owned();
        }
    }

    panic!("package version should be present in Cargo.toml");
}

fn archive_entries(artifact_path: &Path) -> Vec<String> {
    let output = Command::new("tar")
        .args(["-tzf"])
        .arg(artifact_path)
        .output()
        .expect("tar should list archive contents");
    assert_success(&output);

    String::from_utf8(output.stdout)
        .expect("tar output should be utf8")
        .lines()
        .map(str::to_owned)
        .collect()
}

fn checksum_entries(checksums_path: &Path) -> Vec<String> {
    fs::read_to_string(checksums_path)
        .expect("SHA256SUMS should be readable")
        .lines()
        .map(|line| {
            line.split_whitespace()
                .nth(1)
                .unwrap_or_else(|| panic!("checksum line should include a path: {line}"))
                .to_owned()
        })
        .collect()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed with status {}; stdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("test path should be valid UTF-8")
}
