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

## Known Gaps and Risks

- Medium: pseudo-terminal integration tests are skippable when PTY support is unavailable, so some CI/platform combinations may still rely on the runtime harness instead of exercising crossterm through a real terminal.
- Medium: `AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS` is documented as diagnostic-only, but it is still an exported runtime constant and is honored by any binary process whose environment contains it. It currently bypasses `TuiApp::new` clamping with a direct post-construction field assignment so tests can exercise sub-5-second intervals.
- Medium: closed-PTY read-error classification now uses a named Unix EIO constant, but the constant is still locally defined as raw OS error `5` instead of coming from `libc::EIO`, `nix`, or explicit target-specific platform mappings.
- Medium: the CI PTY summary reruns `tui_pty` and `tui_fetch_contract` after the full `cargo test`, increasing CI time and duplicating test execution. This is acceptable for visibility now, but should be revisited if the suite grows.
- Low: PTY helper self-checks live inside `tests/common/pty.rs`, so they are compiled into each integration test crate that imports `mod common`; this is harmless at the current size but duplicates self-check execution and counts.
- Medium: parser priority is correct for key-list precedence and top-level-over-nested precedence, but same-alias duplicate fields inside one JSON object still depend on `serde_json::Map` iteration order. This is acceptable for malformed duplicate-ish payloads, but real-device validation should confirm no important duplicate field variants conflict.
- Medium: non-object top-level config JSON is a documented hard repair boundary because unknown-field preservation requires an object.
- Medium: sensor upper bounds are practical guardrails, not hardware-validated limits; revisit them after real-device validation.
- Medium: installation and release binary basics are documented, but there is still no release workflow, artifact publishing automation, dependency audit policy, or real-device validation record.

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

### Runtime and Test Hook Hygiene

1. Narrow the TUI interval test hook boundary.
   - Decide whether `AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS` should be active only in debug/test builds, hidden behind a less public test-support API, or retained as a diagnostic hook with stronger documentation.
   - Avoid bypassing `TuiApp::new` with a direct post-construction assignment if a cleaner runtime-only scheduling override can preserve the pure app refresh contract.
   - Add focused coverage that unsupported, zero, and lengthening hook values cannot alter production refresh behavior, including the binary path if the hook remains externally settable.

2. Tighten PTY EIO portability.
   - Replace the local raw `5` constant with a platform-provided `EIO` constant or explicit `target_os` mapping whose supported targets are visible in code.
   - Keep the non-Unix rejection test so Windows-like raw error semantics cannot regress.
   - Consider whether the helper should treat unsupported Unix targets conservatively instead of assuming raw error `5` universally means PTY close.

### Release and CI Hygiene

1. Reassess CI PTY summary cost and structure.
   - Keep the current summary step if the duplicated PTY run remains cheap.
   - If the PTY-backed suite grows, split normal tests and PTY summary into clearer steps or use a reusable script so visibility does not require expensive duplicate work.

2. Add dependency and supply-chain checks.
   - Consider `cargo audit` or `cargo deny` with an explicit policy file.
   - Document how failures should be triaged in CI so dependency checks do not become unexplained red builds.

3. Define release automation scope.
   - Decide whether release artifacts are produced manually or by GitHub Actions.
   - If automated, add a release workflow that names Linux artifacts according to the documented target triples.
   - Decide whether shell completion generation is in scope before documenting completion artifacts.

### Validation Hygiene

1. Reassess PTY self-check placement.
   - Keep the current duplicated module tests if the cost remains negligible.
   - If more helper tests are added, move pure helper self-checks into a single integration test target or a small test-support crate to avoid repeated execution.

2. Validate against hardware.
   - Record a real-device validation run when hardware is available.
   - Revisit parser field names, bounds, same-object duplicate alias behavior, and desktop/GNOME compatibility after TUI hardening lands.

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
