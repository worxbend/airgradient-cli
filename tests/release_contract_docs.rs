use std::{fs, path::PathBuf};

const TARGET: &str = "x86_64-unknown-linux-gnu";
const ARTIFACT_PATTERN: &str = "airgradient-cli-v<version>-x86_64-unknown-linux-gnu.tar.gz";
const SCRIPT_ARTIFACT_TEMPLATE: &str = "$BIN_NAME-v$VERSION-$TARGET.tar.gz";
const CHECKSUMS: &str = "SHA256SUMS";

#[test]
fn release_contract_strings_do_not_drift() {
    let readme = read("README.md");
    let boundary = read("docs/release-boundary.md");
    let checklist = read("docs/release-checklist.md");
    let dry_run = read("scripts/release-dry-run.sh");
    let ci = read(".github/workflows/ci.yml");

    assert_release_doc_contract("README.md", &readme);
    assert_release_doc_contract("docs/release-boundary.md", &boundary);
    assert_release_doc_contract("docs/release-checklist.md", &checklist);

    assert_contains("scripts/release-dry-run.sh", &dry_run, TARGET);
    assert_contains(
        "scripts/release-dry-run.sh",
        &dry_run,
        "first-release dry run supports only $DEFAULT_TARGET",
    );
    assert_contains(
        "scripts/release-dry-run.sh",
        &dry_run,
        SCRIPT_ARTIFACT_TEMPLATE,
    );
    assert_contains("scripts/release-dry-run.sh", &dry_run, CHECKSUMS);
    assert_contains("scripts/release-dry-run.sh", &dry_run, "dry run only");
    assert_contains("scripts/release-dry-run.sh", &dry_run, "does not tag");
    assert_contains("scripts/release-dry-run.sh", &dry_run, "publish");
    assert_contains("scripts/release-dry-run.sh", &dry_run, "upload");
    assert_contains("scripts/release-dry-run.sh", &dry_run, "sign");

    assert_contains(
        ".github/workflows/ci.yml",
        &ci,
        "Validate release dry run (validation-only)",
    );
    assert_contains(".github/workflows/ci.yml", &ci, TARGET);
    assert_contains(".github/workflows/ci.yml", &ci, "mktemp -d");
    assert_contains(
        ".github/workflows/ci.yml",
        &ci,
        "${RUNNER_TEMP}/release-dry-run.",
    );
    assert_contains(
        ".github/workflows/ci.yml",
        &ci,
        "Report PTY coverage summary",
    );
    assert_contains(
        ".github/workflows/ci.yml",
        &ci,
        "real pseudo-terminal coverage exercised",
    );
    assert_contains(
        ".github/workflows/ci.yml",
        &ci,
        "PTY unavailable and conditionally skipped",
    );
    assert_contains(
        ".github/workflows/ci.yml",
        &ci,
        "PTY infrastructure failure",
    );
}

fn assert_release_doc_contract(path: &str, contents: &str) {
    assert_contains(path, contents, TARGET);
    assert_contains(path, contents, ARTIFACT_PATTERN);
    assert_contains(path, contents, CHECKSUMS);
    assert_contains(
        path,
        contents,
        "only supported first-release dry-run target",
    );
    assert_contains(path, contents, "validation-only");
    assert_contains_any(
        path,
        contents,
        &["unsupported targets", "every other target"],
    );
    assert_contains(path, contents, "new or empty");
    assert_contains(path, contents, "cargo deny check");
    assert_contains(path, contents, "scripts/release-dry-run.sh --target");
    assert_contains(path, contents, "cargo fmt --check");
    assert_contains(
        path,
        contents,
        "cargo clippy --all-targets --all-features -- -D warnings",
    );
    assert_contains(path, contents, "cargo test");
    assert_contains(path, contents, "PTY coverage");
}

fn assert_contains(path: &str, contents: &str, expected: &str) {
    assert!(
        contents.contains(expected),
        "{path} should contain release-contract string {expected:?}"
    );
}

fn assert_contains_any(path: &str, contents: &str, expected: &[&str]) {
    assert!(
        expected.iter().any(|needle| contents.contains(needle)),
        "{path} should contain one of these release-contract strings: {expected:?}"
    );
}

fn read(path: &str) -> String {
    fs::read_to_string(repo_root().join(path))
        .unwrap_or_else(|error| panic!("{path} should be readable: {error}"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
