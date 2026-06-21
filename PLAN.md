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

Completed in iteration 1:

- Rust binary crate bootstrapped with CLI, HTTP, JSON, color, tracing, and test dependencies.
- Desktop-compatible config struct, default path resolution, read/write round trip, refresh bounds, and config commands implemented.
- Device URL normalization implemented for bare hosts, `http`/`https` URLs, path/query/fragment stripping, and `/measures/current` endpoint construction.
- One-shot fetch path implemented for default command and `fetch`.
- Tolerant sensor parser implemented for current AirGradient field names, common alternates, nested payloads, numeric strings, compensated temperature/humidity preference, and PM2.5 AQI fallback.
- Shared metric presentation spine implemented for labels, units, statuses, formatted values, and trends.
- Text and JSON one-shot output implemented.
- Unit and integration tests cover config path behavior, URL normalization, fetch endpoint, JSON output, missing configured URL, sensor parsing, thresholds, trends, and missing values.

Completed in iteration 2:

- One-shot HTTP fetches use a 5 second default `reqwest` timeout.
- Fetching through an injected `reqwest::Client` is covered so the future TUI can reuse a long-lived client with its own timeout policy.
- Captured non-TTY stdout disables ANSI color by default while preserving explicit `--no-color`.
- `config show` displays a normalized effective `server_url` without rewriting the config file.
- Negative PM2.5, PM1.0, PM10, and PM0.3 count values are treated as missing, including negative numeric strings.
- Invalid PM2.5 no longer feeds the fallback AQI calculation.
- Non-finite numeric strings are ignored.
- The unused `directories` dependency was removed while manual XDG/HOME path resolution remains the chosen implementation.

Completed in iteration 3:

- Default diagnostics are concise, uncolored, and printed by a small local error renderer instead of `color-eyre`.
- `-v` shows source chains; `-vv` also prints debug details and enables trace-level diagnostics.
- Integration tests cover invalid URL, unsupported scheme, non-success HTTP status, invalid JSON, and timeout failures.
- Top-level `--refresh` is rejected unless `--tui` is used; top-level `--json` is rejected for config commands.
- `--tui` remains accepted but explicitly reports that the dashboard is not implemented yet.
- Mutating config commands preserve unknown top-level sibling fields while updating known desktop-compatible fields.
- `config show` now prints malformed or unsupported stored `server_url` values with a warning instead of failing the whole command.
- Explicit AQI is limited to `0..=500`; CO2, TVOC, NOx, and particulate values reject negatives; humidity is limited to `0..=100`.
- README usage and contract documentation was added.
- GitHub Actions CI was added for `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Known gaps and risks:

- TUI is not implemented; `--tui` currently exits with `TUI is not implemented yet.`
- `Cargo.toml` does not yet include `ratatui`, `crossterm`, or `indicatif`; add TUI dependencies only when TUI work begins.
- Timeout duration is still a private one-shot default plus a test/diagnostic environment override. TUI work should expose a small runtime settings boundary and reuse one HTTP client.
- `config show` is tolerant for malformed `server_url`, empty URL, and missing URL, but it still fails if known non-URL fields are malformed, for example an out-of-range `refresh_interval_secs` or wrong JSON type.
- Config mutation preserves unknown top-level fields, but it still requires the known config fields to deserialize and validate before writing; a partially broken shared config may block repair commands.
- Sensor validation still has no defensible upper bounds for CO2, TVOC, NOx, particulate mass/count, or temperature. Lower-bound validation prevents false-good negatives, but absurd high values can still dominate output.
- Parser field-priority behavior is only lightly tested. Nested conflicting fields and real-device payload variants need regression fixtures.
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

### Phase 4A: Pre-TUI Contract Cleanup

1. Extract fetch runtime settings.
   - Introduce a small `FetchSettings` or equivalent boundary for timeout configuration.
   - Provide one client-construction path for CLI and TUI instead of duplicating timeout behavior.
   - Keep `fetch_current_measures_with_client` as the reusable low-level operation.
   - Decide whether `AIRGRADIENT_CLI_FETCH_TIMEOUT_MS` remains documented as diagnostic-only, becomes a supported option, or is moved behind test-only plumbing.

2. Harden config repair and display for partially invalid files.
   - Add tests for `config show` with out-of-range refresh intervals, wrong JSON types for known fields, and non-object top-level JSON.
   - Decide whether `config show` should display a raw/partial view with warnings when known fields fail typed deserialization.
   - Decide whether `config set-url` should be able to repair a bad `server_url` or bad `refresh_interval_secs` while preserving unknown fields.
   - If repair is supported, split raw JSON preservation from typed validation so mutation can update one field without requiring every known field to be valid.

3. Finish sensor-domain policy with realistic upper bounds.
   - Define documented upper bounds or deliberate no-upper-bound decisions for CO2, TVOC, NOx, PM mass, PM0.3 count, and temperature.
   - Add tests for absurdly high values and numeric strings.
   - Ensure invalid explicit AQI, invalid PM2.5, and valid PM2.5 fallback precedence remains correct.

4. Expand parser fixtures and priority tests.
   - Add representative local mock payloads resembling real AirGradient responses.
   - Add regression tests for nested conflicting fields where top-level and nested values disagree.
   - Add tests that compensated temperature/humidity priority still works when the compensated field is invalid but raw field is valid.

### Phase 4B: TUI Dashboard

1. Add TUI dependencies and module layout.
   - `ratatui`
   - `crossterm`
   - `src/tui/mod.rs`
   - `src/tui/app.rs`
   - `src/tui/ui.rs`
   - `src/tui/theme.rs`

2. Implement TUI state machine before terminal drawing.
   - Store current snapshot, previous successful snapshot, last fetch duration, last success timestamp, and current error.
   - Preserve previous successful snapshot on failed fetch.
   - Enforce refresh interval bounds for runtime `+` / `-` changes.
   - Reuse the CLI fetch runtime settings and injected client.

3. Implement Ratatui UI.
   - Top bar: app name, URL, refresh interval, last update status.
   - Main AQI block with status language.
   - Metric grid using the shared presentation spine.
   - Footer with keyboard hints.
   - Error panel for config/fetch failures.

4. Implement event loop.
   - Fetch immediately on startup when URL is available.
   - Refresh on interval.
   - `r` refreshes immediately.
   - `q` and `Esc` quit.
   - Avoid overlapping fetches if a request is in flight.

5. Test TUI logic.
   - Unit-test state transitions without a real terminal.
   - Render smoke-test with Ratatui test backend.
   - Manually verify layout at common terminal sizes.

### Phase 5: Release Hygiene

1. Add packaging and installation notes.
   - Document `cargo install --path .`.
   - Add release binary naming guidance for Linux targets.
   - Decide whether shell completions are in scope.

2. Add dependency and supply-chain checks.
   - Consider `cargo machete` after dependencies stabilize.
   - Consider `cargo audit` or `cargo deny` with an explicit policy file.

3. Validate against hardware.
   - Record a real-device validation run when hardware is available.
   - Revisit compatibility with `airgradient-desktop` and the GNOME extension after TUI lands.

## Acceptance Criteria

- `cargo test` passes.
- `cargo fmt --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- Running `airgradient-cli` fetches once and renders metrics.
- Captured non-TTY output does not include ANSI escapes by default.
- Running `airgradient-cli -t` opens a live Ratatui dashboard.
- The CLI reads the same config file as `airgradient-desktop`.
- `config show` displays a normalized effective config and does not fail solely because a stored URL is malformed.
- `config set-url` updates the desktop-compatible config file while preserving unknown top-level sibling fields.
- Fetching always targets `/measures/current`.
- Missing, partial, or invalid sensor payload values render gracefully without false good statuses.
