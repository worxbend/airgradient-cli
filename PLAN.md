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
- Sensor parsing handles current and alternate AirGradient field names, nested payloads, numeric strings, compensated temperature/humidity, PM2.5 AQI fallback, lower-bound validation, AQI/humidity bounds, and missing-value rendering.
- Tests cover config path behavior, URL normalization, fetch endpoint, output modes, diagnostics, bad HTTP/JSON/timeout cases, flag scope, unknown-field preservation, thresholds, trends, and missing values.

Completed in iteration 4:

- Added `device::FetchSettings` as the shared fetch runtime boundary and routed CLI fetches through one client-construction path.
- Hardened `config show` to parse known fields from raw JSON with warnings for malformed known fields instead of failing typed deserialization.
- Made `config set-url` and `config set-refresh` repair malformed known config fields while preserving unknown top-level sibling fields.
- Added sensor-domain upper bounds for CO2, TVOC, NOx, PM mass, PM0.3 count, and temperature, with tests for absurd high values and numeric strings.
- Added parser fixture coverage for AirGradient-like payloads, alternate names, nested conflicts, missing values, and invalid compensated value fallback.
- Introduced a library crate surface and a pure `tui::app::TuiApp` state model covering current/previous successful snapshots, errors, fetch metadata, refresh interval clamping, and metric trends.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 5:

- Fixed bounded sensor candidate lookup so out-of-range matching fields are skipped and later valid alternate or nested candidates can still populate the metric.
- Added regression coverage for invalid top-level PM2.5, CO2, and AQI fields falling back to valid alternate or nested candidates.
- Preserved valid explicit AQI precedence over PM2.5-derived AQI and valid higher-priority top-level candidate precedence over lower-priority or nested fields.
- Converted `tests/sensor_parsing.rs` to import `airgradient_cli::sensors::parse_snapshot` through the library crate, removing source-path imports and duplicate internal unit-test execution in that integration binary.
- Documented the non-object config repair boundary, parser upper-bound policy, and `AIRGRADIENT_CLI_FETCH_TIMEOUT_MS` as a diagnostic/test hook rather than a supported user-facing setting.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Completed in iteration 6:

- Added `ratatui` and `crossterm` dependencies plus TUI runtime, theme, and rendering modules.
- Replaced the `--tui` placeholder with a real CLI handoff into `tui::runtime::run`.
- Implemented a first Ratatui dashboard using the shared metric presentation spine: top bar, AQI panel, metric grid, error panel, and footer hints.
- Implemented terminal setup with raw mode and alternate screen, immediate fetch on startup when a URL is configured, interval refresh, manual `r` refresh, and `q`/`Esc` quit.
- Reused one HTTP client from `FetchSettings` for TUI refreshes and preserved the last successful snapshot when fetches fail.
- Added Ratatui test-backend smoke tests for missing config, populated snapshots, and error-with-last-success rendering.
- Replaced the CLI integration test that expected the pending TUI error with a minimal assertion that the old placeholder text is gone.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

## Known Gaps and Risks

- High: TUI fetches run inline in the event loop. A slow, hanging, or timing-out request can block `q`, `Esc`, and `r` responsiveness for up to the fetch timeout.
- High: terminal lifecycle tests are shallow. The cleanup unit tests only verify a boolean cleanup plan, not actual call ordering or cleanup after draw/event-loop errors.
- Medium: `TerminalSession::restore` disables raw mode before leaving the alternate screen and showing the cursor. Common terminal cleanup practice is to leave the alternate screen and restore cursor visibility before disabling raw mode, or at least to test the selected order against real terminals.
- Medium: `run` returns the loop error before a cleanup error. This preserves the primary failure, but cleanup failure details can be lost and should be logged or combined if they matter diagnostically.
- Medium: the CLI `--tui` integration test is too weak; it passes if the command fails immediately with a generic terminal error or times out, as long as the old placeholder string is absent.
- Medium: running `--tui` in a non-TTY exits with `error: terminal I/O failed`. That is mechanically acceptable, but the error should be deliberate, clearer, and covered by a test.
- Medium: README still documents `--tui` as unimplemented and says it exits with `TUI is not implemented yet.`
- Medium: render smoke tests cover only a 100x40 backend and string presence. They do not exercise compact terminal sizes, truncation behavior, or layout overlap.
- Medium: parser priority is correct for key-list precedence and top-level-over-nested precedence, but same-alias duplicate fields inside one JSON object still depend on `serde_json::Map` iteration order. This is acceptable for malformed duplicate-ish payloads, but real-device validation should confirm no important duplicate field variants conflict.
- Medium: non-object top-level config JSON is a documented hard repair boundary because unknown-field preservation requires an object.
- Medium: sensor upper bounds are practical guardrails, not hardware-validated limits; revisit them after real-device validation.
- Low: `color-eyre` remains in `Cargo.toml`/`Cargo.lock` even though runtime diagnostics are now local; remove unused dependency surface before release.
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

### Phase 7A: Harden TUI Runtime Correctness

1. Make TUI event handling responsive during fetches.
   - Move refresh work out of the blocking event loop using a small async task/channel boundary or a non-overlapping background fetch state.
   - Keep the existing no-overlap guarantee: skip or coalesce refresh triggers while one fetch is in flight.
   - Keep the 5 second fetch timeout as the backstop, but do not make keyboard responsiveness depend on it.
   - Add tests around refresh scheduling semantics where possible.

2. Strengthen terminal lifecycle handling.
   - Introduce a small terminal adapter abstraction so setup, draw, event polling, and cleanup can be tested without a real terminal.
   - Verify cleanup is attempted after draw errors, event read/poll errors, early loop errors, and normal quit.
   - Revisit cleanup order and explicitly test the chosen order.
   - Preserve the primary runtime error while retaining cleanup failure context through logging, verbose diagnostics, or a combined error type.

3. Make non-TTY `--tui` behavior explicit.
   - Detect non-interactive stdout/stderr before entering raw mode where practical.
   - Return a clear concise error such as `TUI requires an interactive terminal`.
   - Replace the current weak integration test with one that asserts the deliberate non-TTY behavior and exit status.

### Phase 7B: Improve TUI Contract Coverage and Layout Robustness

1. Broaden render smoke tests.
   - Add backends for compact terminal sizes such as 60x20, 80x24, and a narrow mobile-like width.
   - Assert the UI remains nonblank and key labels/statuses are visible or predictably truncated.
   - Add regression coverage for long URLs, long config/fetch errors, and all-missing metrics.

2. Validate dashboard contract against fetch behavior.
   - Add a runtime-level test or testable harness for initial fetch, interval refresh, manual refresh, fetch failure after success, and quit handling.
   - Verify the TUI uses `/measures/current` through the same device boundary as one-shot fetch.
   - Verify URL overrides and refresh overrides take precedence over config values.

3. Update user-facing documentation.
   - Replace README statements that say TUI is unimplemented.
   - Document `--tui`, `--refresh`, keyboard controls, non-TTY limitations, and the missing-URL state.
   - Keep `AIRGRADIENT_CLI_FETCH_TIMEOUT_MS` described as diagnostic/test-only.

### Phase 7C: Dependency and Release Hygiene

1. Clean dependency surface.
   - Remove `color-eyre` if no code path uses it.
   - Run `cargo tree` or `cargo machete` after Ratatui/crossterm are in place to catch stale dependencies.

2. Add packaging and installation notes.
   - Document `cargo install --path .`.
   - Add release binary naming guidance for Linux targets.
   - Decide whether shell completions are in scope.

3. Add dependency and supply-chain checks.
   - Consider `cargo audit` or `cargo deny` with an explicit policy file.

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
