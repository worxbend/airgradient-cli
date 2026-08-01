use std::{fs, path::PathBuf};

const AMD64_TARGET: &str = "x86_64-unknown-linux-gnu";
const ARM64_TARGET: &str = "aarch64-unknown-linux-gnu";
const AMD64_ARTIFACT_PATTERN: &str = "airgradient-cli-v<version>-x86_64-unknown-linux-gnu.tar.gz";
const ARM64_ARTIFACT_PATTERN: &str = "airgradient-cli-v<version>-aarch64-unknown-linux-gnu.tar.gz";
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

    assert_contains("scripts/release-dry-run.sh", &dry_run, AMD64_TARGET);
    assert_contains("scripts/release-dry-run.sh", &dry_run, ARM64_TARGET);
    assert_contains(
        "scripts/release-dry-run.sh",
        &dry_run,
        "release dry run supports only ${SUPPORTED_TARGETS[*]}",
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
    assert_contains(".github/workflows/ci.yml", &ci, AMD64_TARGET);
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

/// Every release doc must name *both* supported Linux triples and both staged
/// artifact names. Asserting the triples rather than one prose sentence is
/// deliberate: the single-target wording this test used to pin went stale when
/// arm64 was added to `scripts/release-dry-run.sh`, so the docs disagreed with
/// the script while the test still passed on the two docs that were updated.
fn assert_release_doc_contract(path: &str, contents: &str) {
    assert_contains(path, contents, AMD64_TARGET);
    assert_contains(path, contents, ARM64_TARGET);
    assert_contains(path, contents, AMD64_ARTIFACT_PATTERN);
    assert_contains(path, contents, ARM64_ARTIFACT_PATTERN);
    assert_contains(path, contents, CHECKSUMS);
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
        flowed(contents).contains(&flowed(expected)),
        "{path} should contain release-contract string {expected:?}"
    );
}

fn assert_contains_any(path: &str, contents: &str, expected: &[&str]) {
    let flowed_contents = flowed(contents);
    assert!(
        expected
            .iter()
            .any(|needle| flowed_contents.contains(&flowed(needle))),
        "{path} should contain one of these release-contract strings: {expected:?}"
    );
}

/// Collapses every run of whitespace to a single space so a contract phrase
/// still matches after a doc is rewrapped. Without this, hard-wrapping a
/// paragraph splits a phrase like "every other target" across a newline and
/// fails the test even though the promise itself never changed.
fn flowed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn read(path: &str) -> String {
    fs::read_to_string(repo_root().join(path))
        .unwrap_or_else(|error| panic!("{path} should be readable: {error}"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
