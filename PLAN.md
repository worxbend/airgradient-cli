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

## Known Gaps and Risks

- Medium: pseudo-terminal integration tests are skippable when PTY support is unavailable, so some CI/platform combinations may still rely on the runtime harness instead of exercising crossterm through a real terminal.
- Medium: binary-level TUI HTTP tests cover startup success/failure, manual refresh, and override precedence, but they do not yet cover interval-triggered refresh with a shortened deterministic clock because the binary enforces the production 5 second minimum.
- Medium: the 36x20 TUI contract guarantees coherent regions and footer access, but it does not guarantee that every metric is visible at the minimum size; the product contract should explicitly decide whether clipped metrics are acceptable or whether scrolling/pagination is required.
- Medium: the CI PTY summary reruns `tui_pty` and `tui_fetch_contract` after the full `cargo test`, increasing CI time and duplicating test execution. This is acceptable for visibility now, but should be revisited if the suite grows.
- Medium: closed-PTY read-error classification treats raw OS error `5` as expected on every platform, while the intended case is Linux/Unix PTY `EIO`; on non-Linux platforms this could hide a real read failure such as access denied.
- Low: PTY helper self-checks live inside `tests/common/pty.rs`, so they are compiled into each integration test crate that imports `mod common`; this is harmless at the current size but duplicates self-check execution and counts.
- Medium: parser priority is correct for key-list precedence and top-level-over-nested precedence, but same-alias duplicate fields inside one JSON object still depend on `serde_json::Map` iteration order. This is acceptable for malformed duplicate-ish payloads, but real-device validation should confirm no important duplicate field variants conflict.
- Medium: non-object top-level config JSON is a documented hard repair boundary because unknown-field preservation requires an object.
- Medium: sensor upper bounds are practical guardrails, not hardware-validated limits; revisit them after real-device validation.
- Low: `color-eyre` remains in `Cargo.toml`/`Cargo.lock` even though runtime diagnostics are local; remove unused dependency surface before release.
- There is no packaging guidance, release workflow, dependency audit, or real-device validation record yet.

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

### Phase 12A: Finish PTY Helper Portability and Evidence Hygiene

1. Scope closed-PTY raw OS error handling by platform.
   - Treat Linux/Unix PTY `EIO` as an expected closed-terminal read only on platforms where that mapping is known.
   - Avoid suppressing raw OS error `5` on Windows or other platforms where it may mean access denied or another real failure.
   - Update the helper self-checks with platform-specific expectations so the test names and implementation match.

2. Tighten skipped-coverage typing.
   - Consider changing `PtyRunResult::Skipped(PtySpawnError)` to a narrower unavailable-only skip type so infrastructure failures cannot accidentally be reintroduced as skipped results by future call sites.
   - Add a small caller-level assertion or helper wrapper that makes the intended unavailable-vs-infrastructure branch hard to misuse.

3. Reassess PTY self-check placement.
   - Keep the current duplicated module tests if the cost remains negligible.
   - If more helper tests are added, move pure helper self-checks into a single integration test target or a small test-support crate to avoid repeated execution.

### Phase 12B: Fill Remaining TUI Contract Gaps

1. Add interval refresh contract coverage.
   - Consider a test-only runtime hook or lower-bound override for interval refresh coverage without adding 5+ second sleeps to more tests.
   - Keep the production `5s` lower bound intact for normal CLI behavior.
   - Prove the binary-level TUI path performs an interval-triggered `/measures/current` request, not only startup and manual refresh.

2. Decide the minimum-size metric visibility contract.
   - Either document that 36x20 may clip lower metric rows while preserving coherent layout and controls, or add scrolling/pagination/alternate compact metric rendering.
   - Add tests for whichever behavior is chosen so the minimum terminal contract is not ambiguous.

3. Reassess CI PTY summary cost and structure.
   - Keep the current summary step if the duplicated PTY run remains cheap.
   - If the PTY-backed suite grows, split normal tests and PTY summary into clearer steps or use a reusable script so visibility does not require expensive duplicate work.

### Phase 12C: Dependency, Release, and Validation Hygiene

1. Clean dependency surface.
   - Remove `color-eyre` if no code path uses it.
   - Run `cargo tree` or `cargo machete` after removal to catch stale dependencies.

2. Add packaging and installation notes.
   - Document `cargo install --path .`.
   - Add release binary naming guidance for Linux targets.
   - Decide whether shell completions are in scope.

3. Add dependency and supply-chain checks.
   - Consider `cargo audit` or `cargo deny` with an explicit policy file.
   - Document how failures should be triaged in CI.

4. Validate against hardware.
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
