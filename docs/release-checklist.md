# Release Checklist

This checklist is for maintainers cutting the first public `airgradient-cli`
release. It is documentation only; release publication remains a manual
maintainer action.

## Release Scope

- [ ] Confirm the release is a binary-only Linux release.
- [ ] Confirm GitHub Actions is being used only as a validation gate, not as the
  release publisher.
- [ ] Confirm no crates.io package, macOS binary, Windows binary, installer,
  package-manager recipe, or shell completion artifact is being promised.

## Version and Tag

- [ ] Bump the project version in `Cargo.toml`.
- [ ] Confirm the committed version is the version being released.
- [ ] Create a version tag using the `vX.Y.Z` form, matching the `Cargo.toml`
  version exactly.

## Validation

Run the release validation commands in this order:

```sh
cargo deny check
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

- [ ] Confirm all validation commands pass with Rust 1.96.0 and cargo-deny
  0.19.9.
- [ ] Record the PTY summary status: real pseudo-terminal coverage exercised, or
  conditionally skipped because usable PTY support was unavailable.
- [ ] Record real-device validation status. If no real AirGradient hardware run
  was performed, add an explicit release-note waiver for this validation gap.

## Release Rehearsal

Run the release artifact dry run before any manual GitHub release upload:

```sh
scripts/release-dry-run.sh --target x86_64-unknown-linux-gnu --output-dir dist
```

- [ ] Confirm the dry run creates
  `dist/airgradient-cli-v<version>-x86_64-unknown-linux-gnu.tar.gz`.
- [ ] Confirm the archive contains the built `airgradient-cli` executable and
  the checked-in `LICENSE` file.
- [ ] Confirm `dist/SHA256SUMS` exists and every checksum entry names a real
  staged release artifact file.
- [ ] Confirm the dry run remains validation-only: no git tag, GitHub release,
  upload, signing, package-manager recipe, shell completion artifact, macOS
  binary, or Windows binary is produced.

## Artifacts

- [ ] Publish Linux binary artifacts only.
- [ ] Name artifacts with versioned, target-explicit Linux filenames, such as
  `airgradient-cli-v<version>-x86_64-unknown-linux-gnu.tar.gz`.
- [ ] Include the checked-in `LICENSE` file inside every release artifact
  bundle.
- [ ] Generate and publish `SHA256SUMS` covering the shipped `.tar.gz` release
  artifact files.
- [ ] Confirm detached signatures are not published or described for this first
  release.

## Documentation Consistency

- [ ] Confirm `README.md` matches the release boundary for installation,
  validation, release rehearsal, PTY coverage, diagnostic test hooks,
  dependency policy, and artifact integrity.
- [ ] Confirm `AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS` is documented as a
  diagnostic/test hook only, scheduler-only, and ignored below 100ms.
- [ ] Confirm PTY closed-read `EIO` handling is documented as target-scoped
  Unix-like behavior using platform-provided `libc::EIO` where available.
- [ ] Confirm `README.md`, `rust-toolchain.toml`, `.github/workflows/ci.yml`,
  and release notes agree on Rust 1.96.0 and cargo-deny 0.19.9.
- [ ] Confirm shell completions are either intentionally absent from release
  notes and artifacts, or actually generated, tested, and packaged before they
  are mentioned in a later release.

## Final Gate

- [ ] Confirm any remaining unresolved item in `docs/release-boundary.md` is
  either fixed, listed as a release blocker, or explicitly accepted in release
  notes as a first-release risk.
- [ ] Confirm the release notes state that the release is manual, Linux-only,
  binary-only, MIT-licensed, checksum-covered with SHA-256, and unsigned.
