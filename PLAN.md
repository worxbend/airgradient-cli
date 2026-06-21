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

## Known Gaps and Risks

- Medium: pending TUI fetches are now aborted and stale results are ignored, but cancellation is not awaited or observed. Cleanup owns cancellation intent, not proof that the spawned task has fully stopped before `run` returns.
- Medium: runtime tests prove behavior through the harness, but there is still no pseudo-terminal integration test that starts the real TUI, sends `q`, and verifies terminal setup/cleanup against crossterm.
- Medium: TUI fetch behavior is mostly harness-tested. There is no end-to-end `--tui --url <server>` test with a real HTTP server proving the runtime path requests `/measures/current`, honors overrides, and handles startup success/failure through the binary.
- Medium: render tests are broader but still mostly assert string presence or deliberate clipping. They do not detect all layout overlap, inaccessible controls, or content loss at very small terminal sizes.
- Medium: parser priority is correct for key-list precedence and top-level-over-nested precedence, but same-alias duplicate fields inside one JSON object still depend on `serde_json::Map` iteration order. This is acceptable for malformed duplicate-ish payloads, but real-device validation should confirm no important duplicate field variants conflict.
- Medium: non-object top-level config JSON is a documented hard repair boundary because unknown-field preservation requires an object.
- Medium: sensor upper bounds are practical guardrails, not hardware-validated limits; revisit them after real-device validation.
- Low: `color-eyre` remains in `Cargo.toml`/`Cargo.lock` even though runtime diagnostics are local; remove unused dependency surface before release.
- There is no packaging guidance, release workflow, dependency audit, pseudo-terminal test coverage, or real-device validation record yet.

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

### Phase 9A: End-to-End TUI Contract Coverage

1. Add pseudo-terminal integration tests.
   - Start the real binary/TUI in a PTY when available.
   - Send `q` and `Esc`, verify process exit, and check that output does not contain the non-TTY error.
   - Include a short timeout and skip gracefully when the platform lacks PTY support.

2. Validate the TUI fetch contract with a real HTTP test server.
   - Exercise `--tui --url <server>` against a local server and assert `/measures/current` is requested.
   - Cover startup success, startup failure, manual refresh, interval refresh, and failure after success where practical.
   - Verify URL overrides and refresh overrides take precedence over config values in the runtime path.
   - Keep the test bounded so a failed TUI cannot hang CI.

3. Tighten background task cancellation semantics.
   - Decide whether `BackgroundMeasureFetcher::cancel_fetch` should await aborted handles, expose an async shutdown method, or document request-only cancellation.
   - Add a runtime test with a pending spawned task that proves the chosen shutdown contract.
   - Surface panicked fetch tasks if they can otherwise be silently detached.

### Phase 9B: Layout and Usability Coverage

1. Strengthen layout assertions.
   - Add coordinate-level or snapshot-style checks for footer controls, top bar status, AQI panel, metric cells, and error panel at compact sizes.
   - Decide and document the minimum supported terminal size.
   - Render a deliberately tiny terminal and verify the app degrades predictably instead of producing incoherent overlap.

2. Review TUI state transitions for stale visible errors.
   - Decide whether old error panels should remain visible during a new in-flight refresh.
   - If old errors remain, make the copy clearly indicate that the app is retrying.
   - Add render tests for refresh-after-failure and failure-after-refresh states.

### Phase 9C: Dependency, Release, and Validation Hygiene

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
