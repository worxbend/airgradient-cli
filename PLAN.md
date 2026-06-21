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
- Verified on 2026-06-21: `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

Known gaps and risks:

- TUI is not implemented; `--tui` currently exits with `TUI is not implemented yet.`
- `Cargo.toml` does not yet include `ratatui`, `crossterm`, or `indicatif`, despite the original stack listing them.
- Non-TTY color suppression is not implemented. Color is disabled only by explicit `--no-color`.
- HTTP fetches have no timeout, so a one-shot command can hang indefinitely on an unreachable or half-open local device.
- `config show` prints the stored URL as-is instead of normalizing the displayed config.
- Config writes emit only the known compatible fields and drop unrelated future fields.
- Negative particulate values are accepted as numeric; PM2.5 AQI fallback turns invalid negative PM2.5 into AQI `0`, which can render as a false "Good" state.
- The `directories` dependency is present but path resolution is implemented manually.
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

## Prioritized Next Work

### Phase 2A: Correctness Hardening

1. Add HTTP timeout behavior.
   - Configure a short default timeout for one-shot fetches.
   - Keep the fetch boundary injectable so TUI can reuse one client.
   - Add a timeout test with a controlled mock or paused runtime where practical.

2. Fix output color policy.
   - Disable ANSI color when stdout is not a terminal unless color is explicitly forced by a future option.
   - Keep `--no-color` as an explicit override.
   - Add an integration test that captures stdout and proves no ANSI escapes are emitted by default in non-TTY execution.

3. Normalize config display and document write semantics.
   - Make `config show` display an effective normalized config when `server_url` is present.
   - Decide whether to preserve unknown JSON fields now or document that writes currently emit the known desktop-compatible shape.
   - Add tests for `config show` with a stored URL containing path/query/fragment.

4. Sanitize sensor numeric domains.
   - Treat impossible negative particulate counts and concentrations as missing, not good.
   - Change AQI fallback so invalid PM2.5 does not become AQI `0`.
   - Add tests for negative PM2.5, negative PM counts, and non-finite JSON-equivalent edge cases.

5. Clean dependency surface.
   - Add `ratatui`, `crossterm`, and any needed TUI dependencies when TUI work begins.
   - Remove `directories` if manual XDG/HOME path resolution remains the chosen approach, or switch config path resolution to `directories` deliberately.
   - Add `cargo machete` or equivalent dependency audit as an optional CI/release check if available.

### Phase 3: CLI Completion

1. Improve user-facing errors.
   - Keep default errors concise.
   - Ensure verbose mode exposes source chains usefully without overwhelming normal command output.
   - Add tests for invalid URL, unsupported scheme, non-success HTTP status, and invalid JSON response.

2. Tighten command semantics.
   - Decide whether top-level `--refresh` should be rejected or ignored for one-shot fetches; document behavior in help text.
   - Confirm `--json` support is limited to fetch/default output and does not imply JSON config command variants.

3. Add README usage examples.
   - Include default fetch, `fetch --json`, `config path`, `config set-url`, `config set-refresh`, `--config`, `--no-color`, and planned `--tui`.

### Phase 4: TUI Dashboard

1. Add TUI dependencies and module layout.
   - `src/tui/mod.rs`
   - `src/tui/app.rs`
   - `src/tui/ui.rs`
   - `src/tui/theme.rs`

2. Implement TUI state machine before terminal drawing.
   - Store current snapshot, previous successful snapshot, last fetch duration, last success timestamp, and current error.
   - Preserve previous successful snapshot on failed fetch.
   - Enforce refresh interval bounds for runtime `+` / `-` changes.

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

### Phase 5: Release Readiness

1. Add CI commands.
   - `cargo fmt --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo test`

2. Add packaging notes for release binaries.

3. Validate against a real AirGradient device or a local mock that mirrors real payloads.

4. Revisit compatibility with `airgradient-desktop` and the GNOME extension after TUI lands.

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
- Fetching always targets `/measures/current`.
- Missing, partial, or invalid sensor payload values render gracefully without false good statuses.
