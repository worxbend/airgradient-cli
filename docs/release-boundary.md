# Release Boundary Audit

Iteration 19 defines the first public release boundary for `airgradient-cli`.
This document is a maintainer-facing audit, not release automation.

## Intended First Release

The first public release is a binary-only Linux release. The repository does
not currently support crates.io publishing because `Cargo.toml` has
`publish = false`, and it does not contain packaging, installer, shell
completion, or multi-platform release automation.

The release owner is a human maintainer. GitHub Actions is currently a
validation gate only: it runs dependency policy, formatting, Clippy, tests, the
release artifact dry run, and a PTY coverage summary. It does not upload, sign,
publish artifacts, or create GitHub releases.

## Shipped Promises

- Binary scope: ship the `airgradient-cli` executable for Linux only.
- Release rehearsal: run the dry-run script before publishing:
  `scripts/release-dry-run.sh --target x86_64-unknown-linux-gnu --output-dir dist`
- Primary artifact name: use the versioned, target-explicit tarball filename:
  `airgradient-cli-v<version>-x86_64-unknown-linux-gnu.tar.gz`.
- Staged dry-run outputs:
  - `dist/airgradient-cli-v<version>-x86_64-unknown-linux-gnu.tar.gz`
  - `dist/SHA256SUMS`
- License inclusion: include the checked-in `LICENSE` file in the release
  artifact bundle. The project license is MIT.
- Checksum generation: generate `SHA256SUMS` over the staged release artifact
  file.
- Validation gate: run the pinned local/CI checks before release:
  - `cargo deny check`
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
- Tool pins: release validation is pinned to Rust 1.96.0 and cargo-deny 0.19.9.
  `README.md`, `rust-toolchain.toml`, and `.github/workflows/ci.yml` must stay
  synchronized when these pins change.
- Dependency policy: `deny.toml` is part of the release gate. Duplicate
  dependency exceptions are allowed only when exact and documented with
  package/version-specific rationale.
- PTY coverage reporting: PTY-backed TUI tests are conditional coverage. A
  release may proceed only when the maintainer records whether PTY tests ran on
  a real pseudo-terminal or were skipped because PTY support was unavailable.
- TUI test refresh hook boundary:
  `AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS` is diagnostic/test support
  only. The accepted contract is scheduler-only behavior: it must not mutate
  `TuiApp` state or the production refresh interval reported by the app model.
  Values below 100ms, zero, invalid values, equal-to-production values, and
  values longer than the production-clamped interval are ignored.
- PTY EIO portability language: expected closed-PTY read errors are a
  target-scoped Unix behavior grounded in platform-provided `libc::EIO` where
  available. Unsupported targets must not treat arbitrary raw OS error `5` as
  normal PTY closure.

## Non-Promises

- No crates.io package is shipped in the first release.
- No Windows or macOS binaries are promised.
- No source tarball beyond the repository snapshot/tag is promised.
- No installer packages, package-manager recipes, desktop entries, service
  files, or distribution-specific packages are promised.
- No shell completions are shipped. Completion artifacts remain out of scope
  until generation and packaging are implemented and tested.
- GitHub Actions does not own release publication.
- No automatic release workflow, artifact upload workflow, signing workflow,
  shell-completion generation, or generated release notes are promised.
- No hardware-compatibility guarantee beyond the documented HTTP/config/parser
  contracts is promised until real-device validation is recorded.

## Artifact Integrity

Decision: publish checksums, do not publish signatures for the first release.

Each Linux binary release must include a SHA-256 checksum file named
`SHA256SUMS` covering the shipped `.tar.gz` release artifact. The rehearsed
artifact is `airgradient-cli-v<version>-x86_64-unknown-linux-gnu.tar.gz`, and
the archive must contain both `airgradient-cli` and the checked-in `LICENSE`.
Detached cryptographic signatures are intentionally out of scope for the first
release.

Release blocker: if the maintainer cannot run the dry run and produce
`SHA256SUMS`, the release must not be cut. Missing detached signatures are an
accepted risk for the first release and must not be described as signed
artifacts.

## Release-Blocking Gaps

- Real-device validation: no real AirGradient hardware validation record is
  present. The first release must either record a real-device run or explicitly
  waive it in release notes as a known validation gap.
- Release rehearsal: `scripts/release-dry-run.sh --target
  x86_64-unknown-linux-gnu --output-dir dist` must succeed before any manual
  release upload.
- Checksum publication: `SHA256SUMS` must be generated and included with the
  release.

## Accepted First-Release Risks

- PTY tests may be skipped on hosts without usable pseudo-terminal support, as
  long as the skip is visible in CI or the release validation record.
- Binary-level refresh-hook tests rely on real PTY processes and bounded
  wall-clock sleeps. This is acceptable at the current test-suite size.
- Duplicate dependency exceptions may remain while they are exact,
  deny-policy-scoped, and documented with rationale.
- Rust 1.96.0 and cargo-deny 0.19.9 pins may become stale between releases. The
  first release accepts pinned reproducibility over floating-tool freshness, but
  future pin updates must run the full validation suite.
- Detached signatures are not provided for the first release. SHA-256 checksums
  are the integrity mechanism.
