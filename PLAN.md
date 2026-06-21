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
- Added parser fixture coverage for AirGradient-like payloads, alternate field names, nested conflicts, missing values, and invalid compensated value fallback.
- Introduced a library crate surface and a pure `tui::app::TuiApp` state model covering current/previous successful snapshots, errors, fetch metadata, refresh interval clamping, and metric trends.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

## Known Gaps and Risks

- High: TUI is not implemented; `--tui` still exits with `TUI is not implemented yet.`
- Medium: sensor candidate selection applies bounds after selecting the first syntactically numeric matching field. An invalid higher-priority field can block a later valid alternate or nested value for the same metric.
- Medium: `tests/sensor_parsing.rs` imports `src/sensors/mod.rs` via `#[path]`, which duplicates module unit tests inside the integration test binary and bypasses the new library crate API.
- Medium: non-object top-level config JSON is still a hard error for display and repair. That is defensible, but it should be documented as the repair boundary.
- Medium: sensor upper bounds are practical guardrails, not hardware-validated limits. They should be documented in README or near parser policy and revisited after real-device validation.
- Low: `FetchSettings` currently only contains timeout. It is sufficient for TUI client reuse, but future retry/backoff or user-facing timeout policy should extend this boundary instead of adding ad hoc fetch options.
- Low: `TuiApp` stores `fetch_settings` and `fetch_client`, but no terminal event loop consumes them yet; the next iteration should either use them directly or trim public state.
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

## Prioritized Next Work

### Phase 5A: Final Pre-TUI Cleanup

1. Fix parser bounded-candidate fallback.
   - Change bounded metric lookup so invalid matching values are skipped and later valid alternates or nested candidates can still be used.
   - Add regression tests for invalid top-level PM/CO2/AQI fields with valid alternate or nested fields.
   - Preserve explicit AQI precedence when valid, and preserve top-level-over-nested precedence when both candidates are valid.

2. Clean integration test architecture.
   - Update `tests/sensor_parsing.rs` to import `airgradient_cli::sensors::parse_snapshot` through the library crate instead of `#[path = "../src/sensors/mod.rs"]`.
   - Remove the `dead_code`/`unused_imports` allowance if it is no longer needed.
   - Confirm the integration test binary no longer reruns internal sensor module unit tests.

3. Document the remaining config and sensor policy boundaries.
   - State that top-level non-object JSON is not repairable because unknown-field preservation requires an object.
   - Document the parser's practical upper bounds and that they are transport/glitch guardrails, not calibrated hardware maxima.
   - Clarify whether `AIRGRADIENT_CLI_FETCH_TIMEOUT_MS` is diagnostic-only or supported user surface.

### Phase 5B: TUI Dashboard

1. Add TUI dependencies and module layout.
   - Add `ratatui` and `crossterm`.
   - Add `src/tui/ui.rs` and `src/tui/theme.rs`.
   - Keep `src/tui/app.rs` as the pure state model and wire it into the runtime.

2. Implement TUI startup and event loop.
   - Resolve config and URL using the existing CLI/config/device contracts.
   - Build one fetch client from `FetchSettings` and reuse it for refreshes.
   - Fetch immediately on startup when URL is available.
   - Refresh on interval; `r` refreshes immediately; `q` and `Esc` quit.
   - Avoid overlapping fetches if a request is already in flight.
   - Surface config/fetch errors without losing the last successful snapshot.

3. Implement Ratatui rendering.
   - Top bar: app name, URL, refresh interval, last update status.
   - Main AQI block with shared status language.
   - Metric grid using the shared presentation spine.
   - Footer with keyboard hints.
   - Error panel for config/fetch failures.
   - Keep layout readable at common terminal sizes.

4. Test TUI behavior.
   - Keep state-transition unit tests.
   - Add render smoke tests with Ratatui's test backend.
   - Add CLI integration coverage that `--tui` no longer returns the pending-implementation error.
   - Manually verify common terminal sizes after the first functional dashboard lands.

### Phase 5C: Release Hygiene

1. Add packaging and installation notes.
   - Document `cargo install --path .`.
   - Add release binary naming guidance for Linux targets.
   - Decide whether shell completions are in scope.

2. Add dependency and supply-chain checks.
   - Consider `cargo machete` after dependencies stabilize.
   - Consider `cargo audit` or `cargo deny` with an explicit policy file.

3. Validate against hardware.
   - Record a real-device validation run when hardware is available.
   - Revisit parser field names, bounds, and desktop/GNOME compatibility after TUI lands.

## Acceptance Criteria

- `cargo test` passes.
- `cargo fmt --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- Running `airgradient-cli` fetches once and renders metrics.
- Captured non-TTY output does not include ANSI escapes by default.
- Running `airgradient-cli -t` opens a live Ratatui dashboard.
- The CLI reads the same config file as `airgradient-desktop`.
- `config show` displays a normalized effective config and does not fail solely because stored known fields are malformed.
- `config set-url` updates the desktop-compatible config file while preserving unknown top-level sibling fields.
- Fetching always targets `/measures/current`.
- Missing, partial, or invalid sensor payload values render gracefully without false good statuses.
