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

- One-shot HTTP fetches now use a 5 second default `reqwest` timeout.
- Fetching through an injected `reqwest::Client` is covered so the future TUI can reuse a long-lived client with its own timeout policy.
- Captured non-TTY stdout disables ANSI color by default while preserving explicit `--no-color`.
- `config show` displays a normalized effective `server_url` without rewriting the config file.
- Config write semantics are explicitly documented in code and tested: mutating config commands currently emit only the known desktop-compatible fields and drop unknown sibling fields.
- Negative PM2.5, PM1.0, PM10, and PM0.3 count values are treated as missing, including negative numeric strings.
- Invalid PM2.5 no longer feeds the fallback AQI calculation.
- Non-finite numeric strings are ignored.
- The unused `directories` dependency was removed while manual XDG/HOME path resolution remains the chosen implementation.
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Known gaps and risks:

- TUI is not implemented; `--tui` currently exits with `TUI is not implemented yet.`
- `Cargo.toml` does not yet include `ratatui`, `crossterm`, or `indicatif`; add TUI dependencies only when TUI work begins.
- Timeout duration is a private constant. That is acceptable for one-shot fetches, but TUI work should expose a small client/fetch configuration boundary instead of duplicating timeout setup.
- Color suppression currently covers rendered stdout text only. Diagnostics from `color-eyre` and tracing may still be styled on stderr unless explicitly configured.
- `config show` now normalizes via the strict device URL parser. A stored invalid or legacy URL makes `config show` fail instead of showing the rest of the config with a warning.
- Config writes still drop unknown future desktop fields by design. This is documented and tested, but preserving unknown fields would be safer for sibling-app compatibility.
- Sensor domain validation remains incomplete outside particulate values. Explicit AQI, CO2, humidity, TVOC, NOx, and temperature still accept any finite number, including physically impossible values.
- There is no README, packaging guidance, CI workflow, or real-device validation record yet.

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

- Use refresh interval limits: minimum `5s`, maximum `3600s`, default `30s`.
- Display AQI, CO2, PM2.5, PM1.0, PM10, PM0.3 count, TVOC, NOx, temperature, and humidity.
- Preserve missing sensor values as missing, rendered as `--` in text and `null` in JSON.
- Show trends when a previous successful reading exists in memory.
- Keep default one-shot output free of ANSI escapes when stdout is captured or piped.

## Prioritized Next Work

### Phase 3A: CLI Contract Completion

1. Improve user-facing error behavior.
   - Keep default errors concise and non-colorized when stderr is not a terminal.
   - Ensure `-v` / `-vv` exposes source chains usefully without overwhelming normal command output.
   - Add integration tests for invalid URL, unsupported scheme, non-success HTTP status, invalid JSON response, and timeout errors.

2. Decide and implement config unknown-field policy.
   - Prefer preserving unknown sibling fields during mutating config commands by reading/writing through a raw JSON object plus typed known fields.
   - If preservation is deferred, document known-field-only writes in README and command help, not only in a code comment.
   - Add tests that mutation preserves unknown fields or intentionally drops them with user-visible documentation.

3. Make `config show` robust for imperfect existing files.
   - Decide whether a malformed stored `server_url` should fail the entire command or print the raw value plus a warning.
   - Add tests for unsupported scheme, malformed URL, empty string, and missing URL.

4. Tighten command semantics and help text.
   - Decide whether top-level `--refresh` should be rejected, ignored, or documented as TUI-only until TUI exists.
   - Decide whether top-level `--json` should apply only to fetch/default output and make config commands reject or ignore it consistently.
   - Consider hiding or documenting `--tui` as pending until the dashboard exists.

5. Complete sensor domain validation.
   - Validate explicit AQI as non-negative and within a defensible upper bound or clamp/report as missing.
   - Treat negative CO2, humidity outside `0..=100`, negative TVOC/NOx indexes, and impossible particulate counts as missing.
   - Add tests showing invalid explicit AQI cannot override a valid PM2.5 fallback with a misleading negative status.

### Phase 3B: Documentation and Release Hygiene

1. Add README usage examples.
   - Include default fetch, `fetch --json`, `config path`, `config show`, `config set-url`, `config set-refresh`, `--config`, `--no-color`, and planned `--tui`.
   - Explicitly describe config file compatibility and current unknown-field write behavior.
   - Document the default fetch timeout and how failures are reported.

2. Add CI commands.
   - `cargo fmt --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo test`
   - Optionally add a dependency audit such as `cargo machete` once the project has a stable CI setup.

3. Expand fetch and parser validation.
   - Add local mock payloads that resemble real AirGradient responses.
   - Add regression tests for nested conflicting fields and field-priority behavior.
   - Record a real-device validation run when hardware is available.

### Phase 4: TUI Dashboard

1. Add TUI dependencies and module layout.
   - `ratatui`
   - `crossterm`
   - `src/tui/mod.rs`
   - `src/tui/app.rs`
   - `src/tui/ui.rs`
   - `src/tui/theme.rs`

2. Extract reusable fetch runtime settings before the event loop.
   - Provide a single place to build the HTTP client and configure timeout behavior.
   - Reuse `fetch_current_measures_with_client` from the TUI.
   - Avoid creating a new HTTP client on every TUI refresh.

3. Implement TUI state machine before terminal drawing.
   - Store current snapshot, previous successful snapshot, last fetch duration, last success timestamp, and current error.
   - Preserve previous successful snapshot on failed fetch.
   - Enforce refresh interval bounds for runtime `+` / `-` changes.

4. Implement Ratatui UI.
   - Top bar: app name, URL, refresh interval, last update status.
   - Main AQI block with status language.
   - Metric grid using the shared presentation spine.
   - Footer with keyboard hints.
   - Error panel for config/fetch failures.

5. Implement event loop.
   - Fetch immediately on startup when URL is available.
   - Refresh on interval.
   - `r` refreshes immediately.
   - `q` and `Esc` quit.
   - Avoid overlapping fetches if a request is in flight.

6. Test TUI logic.
   - Unit-test state transitions without a real terminal.
   - Render smoke-test with Ratatui test backend.
   - Manually verify layout at common terminal sizes.

### Phase 5: Release Readiness

1. Add packaging notes for release binaries.

2. Validate against a real AirGradient device or a local mock that mirrors real payloads.

3. Revisit compatibility with `airgradient-desktop` and the GNOME extension after TUI lands.

## Acceptance Criteria

- `cargo test` passes.
- `cargo fmt --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- Running `airgradient-cli` fetches once and renders metrics.
- Captured non-TTY output does not include ANSI escapes by default.
- Running `airgradient-cli -t` opens a live Ratatui dashboard.
- The CLI reads the same config file as `airgradient-desktop`.
- `config show` displays a normalized effective config.
- `config set-url` updates the desktop-compatible config file.
- Mutating config commands have an explicit, user-visible unknown-field policy.
- Fetching always targets `/measures/current`.
- Missing, partial, or invalid sensor payload values render gracefully without false good statuses.
