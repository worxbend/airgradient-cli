# AirGradient CLI/TUI Implementation Plan

## Goal

Build a Rust command-line companion for the existing AirGradient desktop and GNOME extension workflow.

The binary must support:

- Default CLI mode: fetch the current AirGradient reading once and render compact terminal output.
- TUI mode via `-t` / `--tui`: open an auto-refreshing Ratatui dashboard with the same information density and status language as the desktop app and GNOME extension.

The CLI must read and write the same configuration file used by `airgradient-desktop`:

```text
$XDG_CONFIG_HOME/airgradient-desktop/config.json
```

If `XDG_CONFIG_HOME` is not set, use:

```text
$HOME/.config/airgradient-desktop/config.json
```

## Current Status

Completed in iterations 1-3:

- Rust binary crate bootstrapped with CLI parsing, HTTP/JSON support, config commands, one-shot fetch, text/JSON output, concise diagnostics, README contract documentation, and GitHub Actions CI.
- Desktop-compatible config path, shape, refresh bounds, URL normalization, `/measures/current` endpoint construction, and unknown top-level field preservation are implemented.
- One-shot fetches use a default 5 second timeout, captured stdout suppresses ANSI color by default, and default diagnostics are concise/uncolored with `-v`/`-vv` detail.
- Sensor parsing handles current and alternate AirGradient field names, nested payloads, numeric strings, compensated temperature/humidity, PM2.5 AQI fallback, domain validation, upper-bound guardrails, and missing-value rendering.
- Tests cover config path behavior, URL normalization, fetch endpoint, output modes, diagnostics, bad HTTP/JSON/timeout cases, flag scope, unknown-field preservation, thresholds, trends, and missing values.

Completed in iterations 4-5:

- Added `device::FetchSettings` as the shared fetch runtime boundary and routed CLI fetches through one client-construction path.
- Hardened `config show`, `config set-url`, and `config set-refresh` to tolerate malformed known config fields while preserving unknown top-level sibling fields.
- Fixed bounded sensor candidate lookup so out-of-range matching fields are skipped and later valid alternate or nested candidates can still populate the metric.
- Added parser fixture coverage for AirGradient-like payloads, alternate names, nested conflicts, missing values, invalid compensated value fallback, fallback candidate priority, and public library API usage.
- Introduced a library crate surface and a pure `tui::app::TuiApp` state model covering current/previous successful snapshots, errors, fetch metadata, refresh interval clamping, and metric trends.
- Documented the non-object config repair boundary, parser upper-bound policy, and `AIRGRADIENT_CLI_FETCH_TIMEOUT_MS` as a diagnostic/test hook.

Completed in iteration 6:

- Added `ratatui` and `crossterm` dependencies plus TUI runtime, theme, and rendering modules.
- Replaced the `--tui` placeholder with a real CLI handoff into `tui::runtime::run`.
- Implemented a first Ratatui dashboard using the shared metric presentation spine: top bar, AQI panel, metric grid, error panel, and footer hints.
- Implemented terminal setup with raw mode and alternate screen, immediate fetch on startup when a URL is configured, interval refresh, manual `r` refresh, and `q`/`Esc` quit.
- Reused one HTTP client from `FetchSettings` for TUI refreshes and preserved the last successful snapshot when fetches fail.
- Added Ratatui test-backend smoke tests for missing config, populated snapshots, and error-with-last-success rendering.

Completed in iteration 7:

- Introduced a `TerminalRuntime` adapter and `MeasureFetchWorker` boundary so the TUI loop can be tested without a real terminal.
- Moved TUI fetches out of the blocking event loop using a background task and channel, with coalescing so manual/interval refresh triggers do not overlap an in-flight fetch.
- Added runtime harness tests for initial fetch, interval refresh, manual refresh coalescing, fetch failure after success, pending-fetch quit, normal quit, draw/poll/read errors, cleanup failure, and retained cleanup context.
- Changed terminal cleanup order to leave the alternate screen, show the cursor, then disable raw mode; setup-failure cleanup uses the same ordering.
- Made non-TTY `--tui` failure deliberate with `TUI requires an interactive terminal` before raw-mode setup, and replaced the weak CLI integration test with an exit-status/error assertion.
- Broadened render smoke tests across 60x20, 80x24, and 36x20 backends, long URLs, long config/fetch errors, and all-missing metrics.
- Updated README TUI documentation for `--tui`, `--refresh`, keyboard controls, non-TTY limitations, missing URL state, and URL overrides.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 8:

- Simplified the public `TuiApp` contract so render state no longer owns `FetchSettings` or a `reqwest::Client`; fetching now stays behind the runtime worker boundary.
- Added explicit in-flight state to `TuiApp` with `begin_fetch`, `finish_fetch_success`, and `finish_fetch_failure`, preserving the last successful snapshot while a refresh is pending.
- Rendered visible TUI states for `fetching`, `refreshing`, `waiting for first fetch`, `updated ...`, `fetch failed`, and missing config.
- Made background fetch ownership explicit with request ids and a stored `JoinHandle`; pending fetches are aborted on quit and runtime errors, and stale completions after cancellation are ignored.
- Changed terminal polling and event reads to use `tokio::task::spawn_blocking`, allowing spawned fetch work to progress even under a current-thread Tokio runtime.
- Reworked refresh scheduling to sample wall-clock `Instant::now()` through a clock abstraction instead of advancing synthetic time by requested poll durations.
- Added runtime harness coverage for pending fetch cancellation, stale completion discard, current-thread runtime progress while terminal polling blocks, early false polls, delayed polls, and manual-refresh interval resets.
- Added render coverage for initial `fetching`, active `refreshing`, and missing-config states.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 9:

- Tightened TUI shutdown cancellation semantics so pending background fetches are aborted and their task handles are awaited before the runtime returns.
- Added runtime harness coverage for quitting while a spawned fetch task is pending, stale completion discard after cancellation, and panicked fetch task surfacing when completion is observed.
- Added skippable pseudo-terminal integration tests that start the real `--tui` binary path and verify both `q` and `Esc` exit without the non-TTY error.
- Added binary-level TUI HTTP contract tests proving startup success and startup failure request `<server_url>/measures/current`, manual `r` refresh requests it again, and CLI `--url`/`--refresh` overrides take precedence over config-file values.
- Documented the TUI contract for the interactive-terminal requirement, `q`/`Esc` exit behavior, fetch endpoint, override precedence, terminal cleanup, last-success retention after later failures, and awaited cancellation guarantee.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 10:

- Defined a documented compact TUI layout contract: dashboard panels render at 36x20 and larger, while smaller terminals show a resize/fallback message instead of overlapping dashboard panels.
- Added coordinate-level render assertions for the top bar, AQI panel, metric grid, footer controls, error panel, compact supported sizes, and below-minimum fallback rendering.
- Clarified retry-after-error visible state: stale errors remain visible during retry, but the top status says `retrying`, the error title changes to `Retrying After Error`, and copy explains that the previous error is being retried.
- Preserved secondary TUI shutdown failure context when a primary draw/poll/read error races with fetch cancellation or a fetch-task panic, using a new `RuntimeError::Secondary` wrapper and harness tests.
- Made conditional PTY coverage more explicit in README and test skip messages.
- Validation on 2026-06-21: `cargo test` and `cargo clippy --all-targets --all-features -- -D warnings` pass, but `cargo fmt --check` fails due import ordering in `tests/tui_fetch_contract.rs` and `tests/tui_pty.rs`.

Completed in iteration 11:

- Restored the formatting gate; `cargo fmt --check` now passes again.
- Extracted shared PTY integration-test helpers into `tests/common/pty.rs`, covering PTY spawn, input writing, output draining, timeout handling, cleanup, and conditional skip reporting.
- Updated `tests/tui_pty.rs` and `tests/tui_fetch_contract.rs` to use the shared `PtyTui` helper and centralized skip-reporting function.
- Added a GitHub Actions summary step that reruns the PTY-backed TUI tests with `--nocapture` and reports whether a real pseudo-terminal was exercised or conditionally skipped.
- Documented the CI summary behavior in README.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 12:

- Introduced typed PTY spawn failures with `PtySpawnError::{Unavailable, Infrastructure}`.
- Kept `openpty` failures as skippable platform-capability gaps while treating missing `CARGO_BIN_EXE_airgradient-cli`, invalid binary paths, PTY reader/writer setup failures, and child spawn failures as test infrastructure errors.
- Updated both PTY-backed integration test files so infrastructure errors panic instead of being reported as skipped conditional coverage.
- Changed the PTY output reader to send chunks or retained read errors, ignore expected closed-PTY conditions, and fail tests on unexpected read errors with captured output and child status context.
- Added PTY helper self-checks for closed-PTY error classification and typed spawn-error formatting/branching.
- Updated the GitHub Actions PTY summary wording so infrastructure failures, PTY-unavailable skips, and real PTY exercise are reported distinctly.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 13:

- Scoped PTY closed-read raw OS error handling away from non-Unix platforms, so Windows raw OS error `5` is no longer silently treated as an expected closed-PTY read.
- Narrowed skipped PTY run results to `PtyRunResult::Skipped(PtyUnavailable)` and added `PtyTui::spawn_or_skip` so infrastructure spawn failures panic instead of being representable as conditional coverage skips.
- Removed the unused `color-eyre` dependency from `Cargo.toml` and pruned its transitive packages from `Cargo.lock`; `cargo tree -i color-eyre` no longer resolves a package.
- Documented local installation with `cargo install --path .`, Linux release artifact naming expectations, and the current absence of shell completion artifacts.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 14:

- Added `AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS` as a diagnostic-only hook for binary-level TUI tests to shorten interval refresh timing without changing the production refresh bounds.
- Preserved normal config and CLI refresh validation at the documented `5s` minimum and `3600s` maximum, with focused coverage proving the test hook is applied only after the production interval is clamped.
- Added binary-level PTY coverage proving interval-triggered TUI refresh requests `<server_url>/measures/current` again without manual `r` input, separate from startup and manual-refresh coverage.
- Made the 36x20 compact TUI metric visibility contract explicit: coherent panels, status, footer controls, and priority metrics are preserved, while lower metric rows may be clipped by design without scrolling or pagination.
- Replaced the raw PTY closed-read OS error literal at the comparison site with a named Unix EIO mapping and retained coverage that Windows-like raw error semantics are rejected as expected PTY closure.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 15:

- Refactored `AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS` so it no longer mutates `TuiApp::refresh_interval` after construction. The app model still reports and enforces the production-clamped interval, while the env hook only shortens the runtime scheduler interval.
- Added runtime coverage that missing, invalid, zero, equal, and lengthening refresh-hook values keep the production schedule, and that a valid shorter hook triggers interval refresh without changing the app interval.
- Added binary-level PTY HTTP coverage proving unsupported hook values do not cause an early second `/measures/current` request.
- Replaced the single Unix EIO comparison constant with explicit target-scoped EIO mappings and tests for supported and unsupported targets.
- Updated README documentation for the scheduler-only hook boundary and target-scoped PTY closed-read classification.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 16:

- Added a checked-in `cargo-deny` policy covering advisories, yanked crates, duplicate versions, wildcard dependencies, license allowlisting, and unknown registry/git sources.
- Added exact duplicate-version exemptions with package/version-specific rationale for the current dependency graph instead of broad blanket allowances.
- Wired `cargo deny check` into GitHub Actions before formatting, Clippy, and tests.
- Documented maintainer triage expectations for advisories, yanked crates, duplicate dependency versions, license failures, and unknown sources.
- Marked the crate `publish = false`, making the current package metadata consistent with local/binary release guidance rather than accidental crates.io publishing.
- Verified on 2026-06-21: `cargo deny check`, `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 17:

- Added an MIT `LICENSE` file and matching `license = "MIT"` package metadata.
- Kept `publish = false` and documented the current release scope as binary-only and manually released, with crates.io publishing deferred until an explicit packaging decision.
- Added `rust-toolchain.toml` pinning Rust 1.96.0 with `rustfmt` and `clippy`, aligning local validation with CI.
- Pinned GitHub Actions validation to Rust 1.96.0 and cargo-deny 0.19.9 instead of moving stable/latest installer behavior.
- Documented release validation order and tool versions in README.
- Verified on 2026-06-21: `cargo deny check`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` pass.

Completed in iteration 18:

- Defined the first-release boundary in `docs/release-boundary.md`: manual, binary-only Linux release; GitHub Actions remains validation-only; no crates.io, macOS/Windows binaries, installers, package-manager recipes, shell completions, signatures, or release automation are promised.
- Added `docs/release-checklist.md` covering release scope, version/tag matching, pinned validation commands, PTY coverage recording, real-device validation status, target-explicit artifact names, license inclusion, checksum publication, documentation consistency, and final release-note requirements.
- Updated README release guidance to require target-explicit Linux artifact names, MIT license inclusion, `SHA256SUMS` publication, unsigned first-release wording, PTY coverage recording, real-device validation status, tool-pin update discipline, and duplicate-dependency exception pruning.
- Added a 100ms minimum floor for `AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS`, keeping the hook scheduler-only and ignored for invalid, zero, below-floor, equal-to-production, and longer-than-production values.
- Replaced target-scoped local PTY EIO raw constants with platform-provided `libc::EIO` in PTY helper tests, while preserving conservative unsupported-target behavior.
- Expanded binary PTY HTTP coverage so a below-floor refresh-hook value does not trigger an early second `/measures/current` request.
- Verified on 2026-06-21: `cargo deny check`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` pass.

Completed in iteration 19:

- Added `scripts/release-dry-run.sh`, a maintainer dry-run command that builds or stages the Linux release artifact, packages `airgradient-cli` with the checked-in `LICENSE`, and writes `SHA256SUMS`.
- Standardized the first-release artifact shape around `airgradient-cli-v<version>-x86_64-unknown-linux-gnu.tar.gz` plus `SHA256SUMS`, with checksum coverage over the staged tarball.
- Added `tests/release_dry_run.rs` covering skip-build staging, archive contents, checksum manifest entries, and explicit missing-binary failures.
- Wired the release artifact dry run into GitHub Actions as validation-only behavior, without upload, signing, tagging, or release publication.
- Updated README, `docs/release-boundary.md`, and `docs/release-checklist.md` to document the release rehearsal workflow and the versioned tarball artifact contract.
- Targeted verification on 2026-06-21: `cargo test --test release_dry_run` passes, and manual skip-build probes produced the expected documented tarball/checksum for `x86_64-unknown-linux-gnu`.

## Known Gaps and Risks

- Medium: pseudo-terminal integration tests are skippable when PTY support is unavailable, so some CI/platform combinations may still rely on the runtime harness instead of exercising crossterm through a real terminal.
- Medium: `AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS` no longer mutates app state and now has a 100ms floor, but it is still a public exported constant and is honored by normal binary processes. Accidental production environments can still force faster-than-documented polling.
- Low: closed-PTY read-error classification now uses platform-provided `libc::EIO` on supported Unix-like targets, but the supported-target list remains a local policy decision that should stay conservative as more platforms are considered.
- Medium: binary-level refresh-hook coverage uses real PTY processes and wall-clock sleeps. The bounded sleeps are acceptable now, but the suite will get slower if more timing cases are added without a deterministic binary-test seam.
- Medium: the CI PTY summary reruns `tui_pty` and `tui_fetch_contract` after the full `cargo test`, increasing CI time and duplicating test execution. This is acceptable for visibility now, but should be revisited if the suite grows.
- Low: PTY helper self-checks live inside `tests/common/pty.rs`, so they are compiled into each integration test crate that imports `mod common`; this is harmless at the current size but duplicates self-check execution and counts.
- Medium: parser priority is correct for key-list precedence and top-level-over-nested precedence, but same-alias duplicate fields inside one JSON object still depend on `serde_json::Map` iteration order. This is acceptable for malformed duplicate-ish payloads, but real-device validation should confirm no important duplicate field variants conflict.
- Medium: non-object top-level config JSON is a documented hard repair boundary because unknown-field preservation requires an object.
- Medium: sensor upper bounds are practical guardrails, not hardware-validated limits; revisit them after real-device validation.
- Medium: first-release scope, version/tag expectations, checksum policy, signing non-scope, and release checklist are now documented, but the project still has no release workflow, artifact publishing automation, checksum-generation helper, or real-device validation record.
- Medium: Rust and cargo-deny are now pinned, but future updates need a documented cadence so security/tooling updates are deliberate instead of stale.
- Medium: duplicate-version deny exceptions are exact and rationalized, but they can become stale release-policy noise if upstream dependency convergence is not periodically checked.
- Medium: the release dry-run script accepts arbitrary target triples, including non-Linux targets when `--skip-build` is used, even though the first-release boundary is Linux-only and currently documents `x86_64-unknown-linux-gnu`.
- Medium: the release dry-run output directory is not required to be empty and is not cleaned, so stale tarballs from earlier rehearsals can remain next to the newly generated artifact and confuse manual publication.
- Medium: README says local release validation should follow CI order, but the listed local commands omit the CI dry-run and PTY summary step; the release checklist documents the dry run separately, so the validation story is currently split.
- Low: `--skip-build` only checks that the supplied binary path exists; it does not verify executable permissions on Unix. This is acceptable for the current test helper path but weaker than the release artifact contract.

## Compatibility Targets

Must preserve:

- Fetch `<server_url>/measures/current`.
- Accept bare hosts such as `192.168.1.201` and normalize them to `http://192.168.1.201/`.
- Strip path, query, and fragment before saving the base URL.
- Reject unsupported schemes; only `http` and `https` are valid.
- Support the shared config JSON shape:

```json
{
  "server_url": "http://192.168.1.201",
  "refresh_interval_secs": 30,
  "notifications_enabled": true,
  "start_minimized": false
}
```

- Preserve unknown top-level sibling fields when mutating known config values.
- Use refresh interval limits: minimum `5s`, maximum `3600s`, default `30s`.
- Display AQI, CO2, PM2.5, PM1.0, PM10, PM0.3 count, TVOC, NOx, temperature, and humidity.
- Preserve missing sensor values as missing, rendered as `--` in text and `null` in JSON.
- Show trends when a previous successful reading exists in memory.
- Keep default one-shot output free of ANSI escapes when stdout is captured or piped.
- Keep default diagnostics concise and uncolored; use verbosity for source chains and debug details.
- Restore terminal state after TUI exit and after runtime errors once terminal setup has started.

## Prioritized Next Work

### Release and CI Readiness

1. Harden the release artifact dry-run contract.
   - Reject unsupported target triples up front; the first-release dry run should allow only documented Linux targets, currently `x86_64-unknown-linux-gnu`.
   - Add tests proving non-Linux targets and unsupported Linux targets fail before staging artifacts, including the `--skip-build` path.
   - Decide whether the output directory must be empty, cleaned, or target-version scoped; prevent stale tarballs from being mistaken for current release artifacts.
   - Verify or enforce executable permissions for `--skip-build` binaries on Unix so test fixture shortcuts cannot produce a non-executable packaged binary.

2. Align release validation documentation with CI.
   - Update README and `docs/release-checklist.md` so the local release-validation order includes `cargo deny check`, the release dry run, formatting, Clippy, tests, and PTY coverage reporting exactly as CI presents them.
   - Clarify that the dry run is validation-only and may be run with a temporary output directory in CI, while maintainers should use a clean release staging directory.
   - Add a small docs consistency check or release checklist item that prevents drift between CI, README, and release-boundary dry-run commands.

3. Add a tool-update policy.
   - Document how and when to update Rust 1.96.0 and cargo-deny 0.19.9 pins.
   - Require the full validation suite after tool updates because rustfmt, Clippy, and cargo-deny defaults can change release outcomes.
   - Keep README, `rust-toolchain.toml`, and GitHub Actions pins synchronized.

4. Tighten dependency-policy maintenance.
   - Periodically re-run `cargo tree -d --target all` and prune exact duplicate-version skips when upstream dependencies converge.
   - Investigate whether `comfy-table`/`ratatui`/direct `crossterm` usage can be aligned to one `crossterm` line in a future dependency update.
   - Keep advisory ignores and license exceptions empty unless a specific release decision adds a narrowly documented exception.

5. Decide shell-completion scope.
   - Either keep completions explicitly out of the first release or add generation and artifact packaging.
   - Avoid documenting completion artifacts until generation is implemented and tested.

6. Keep release-boundary docs synchronized.
   - Treat `README.md`, `docs/release-boundary.md`, `docs/release-checklist.md`, `Cargo.toml`, `rust-toolchain.toml`, `.github/workflows/ci.yml`, and release notes as one release contract.
   - Add a lightweight checklist review before release so artifact names, checksum policy, signing language, license inclusion, PTY coverage state, and real-device validation status do not drift.

### Test Infrastructure Hygiene

1. Reassess CI PTY summary cost and structure.
   - Keep the current summary step if the duplicated PTY run remains cheap.
   - If the PTY-backed suite grows, split normal tests and PTY summary into clearer steps or use a reusable script so visibility does not require expensive duplicate work.

2. Further contain the TUI interval hook if release hardening requires it.
   - Consider making the exported constant private and duplicating the literal only in integration tests, or move the hook behind an internal test-support boundary.
   - If retained in all builds, explicitly accept the 100ms floor as a diagnostic tradeoff and keep binary coverage for ignored below-floor values.
   - Keep the invariant that the hook affects only runtime scheduling and never mutates `TuiApp` state.

3. Reassess PTY helper self-check placement.
   - Keep the current duplicated module tests if the cost remains negligible.
   - If more helper tests are added, move pure helper self-checks into a single integration test target or a small test-support crate to avoid repeated execution.

### Validation Hygiene

1. Validate against hardware.
   - Record a real-device validation run when hardware is available.
   - Revisit parser field names, bounds, same-object duplicate alias behavior, and desktop/GNOME compatibility after TUI hardening lands.
   - If the first release ships without hardware access, document the waiver explicitly in release notes as required by `docs/release-boundary.md`.

## Acceptance Criteria

- `cargo test` passes.
- `cargo fmt --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- Running `airgradient-cli` fetches once and renders metrics.
- Captured non-TTY output does not include ANSI escapes by default.
- Running `airgradient-cli -t` opens a live Ratatui dashboard in an interactive terminal.
- TUI keyboard controls remain responsive while fetches are pending or failing.
- TUI exits restore terminal raw mode, alternate screen, and cursor visibility on normal and error paths.
- The CLI reads the same config file as `airgradient-desktop`.
- `config show` displays a normalized effective config and does not fail solely because stored known fields are malformed.
- `config set-url` updates the desktop-compatible config file while preserving unknown top-level sibling fields.
- Fetching always targets `/measures/current`.
- Missing, partial, or invalid sensor payload values render gracefully without false good statuses.
