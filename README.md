# AirGradient CLI

`airgradient-cli` is a command-line companion for an AirGradient device. By
default it reads the shared desktop config, fetches the current measurements
once, and prints compact terminal output.

The interactive TUI dashboard is available with `-t` / `--tui` in an
interactive terminal.

## Installation and Releases

For local installation from a checked-out repository, run:

```sh
cargo install --path .
```

This installs the expected binary name, `airgradient-cli`, into Cargo's binary
directory.

This repository is currently maintained as a binary-only, manually released
crate. `Cargo.toml` intentionally keeps `publish = false`, so the package is
not intended for crates.io publishing yet. Future crates.io publishing requires
an explicit packaging decision before removing `publish = false`.

The repository license is MIT. Binary redistribution should keep the checked-in
MIT license text with the release artifacts.

Linux release artifacts should make the target platform clear in the filename,
for example `airgradient-cli-x86_64-unknown-linux-gnu` or
`airgradient-cli-aarch64-unknown-linux-gnu`. The repository does not currently
ship shell completions, so release artifacts should not list completions unless
completion generation support is added first.

## Release Validation

Release validation is pinned to Rust 1.96.0 and cargo-deny 0.19.9. Maintainers
should run the local checks in the same order as CI:

```sh
cargo deny check
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Dependency Policy

This project uses `cargo-deny` as a dependency and supply-chain release gate.
Maintainers should triage `cargo deny check` failures before cutting a release:

- Known vulnerabilities and yanked crates are release blockers unless they are
  consciously patched, replaced, or otherwise handled with a documented reason.
- Duplicate dependency versions should be collapsed when reasonable. If a
  transitive duplicate cannot be removed in the current change, exempt only the
  specific package/version case and include the rationale in the policy.
- License failures require confirming that the license is compatible with the
  project before extending the allowlist.
- Unknown registries or git sources require explicit review before they are
  allowed.

## Usage

Configure the AirGradient device URL once:

```sh
airgradient-cli config set-url 192.168.1.201
```

Then fetch the current reading:

```sh
airgradient-cli
```

The explicit fetch command does the same one-shot request:

```sh
airgradient-cli fetch
```

Print the one-shot reading as JSON:

```sh
airgradient-cli fetch --json
```

The top-level `--json` flag also applies to one-shot fetch output:

```sh
airgradient-cli --json
```

`--json` does not apply to config commands. For example,
`airgradient-cli --json config show` exits with an error instead of silently
changing the command output contract.

Show the config path:

```sh
airgradient-cli config path
```

Show the effective config:

```sh
airgradient-cli config show
```

Set the device URL:

```sh
airgradient-cli config set-url http://192.168.1.201/some/path?ignored=true
```

Set the refresh interval stored for the dashboard:

```sh
airgradient-cli config set-refresh 30
```

Use a different config file for one command:

```sh
airgradient-cli --config ./airgradient-config.json config show
```

Disable color explicitly in text output:

```sh
airgradient-cli --no-color
```

Start the interactive dashboard:

```sh
airgradient-cli --tui
```

The TUI opens a Ratatui dashboard using the same AirGradient config file as the
desktop app. It fetches `<server_url>/measures/current` on startup when a device
URL is configured, then refreshes on the configured interval. Press `r` to
refresh manually, or press `q` or `Esc` to quit. If a later refresh fails after a
successful reading, the dashboard keeps showing the last successful snapshot
alongside the error.

The TUI requires an interactive terminal. Running `--tui` with captured or
piped terminal streams exits with `TUI requires an interactive terminal`.
The dashboard supports terminals at least 36 columns by 20 rows. Smaller
terminal windows show a compact resize message instead of the dashboard panels.
At the minimum supported 36x20 size, the layout preserves coherent panels,
status text, keyboard controls, and the priority AQI reading, but lower metric
rows may be clipped by design; the TUI does not add scrolling or pagination for
that compact layout.
On exit and runtime error paths after terminal setup starts, the TUI restores
terminal state by leaving the alternate screen, showing the cursor, and
disabling raw mode.

Override the TUI refresh interval for one run:

```sh
airgradient-cli --tui --refresh 10
```

`--refresh <SECONDS>` applies only to `--tui` and is rejected for one-shot
fetches and config commands. Like `config set-refresh`, it accepts values from
`5` to `3600` seconds. It changes only the current dashboard run and does not
write the config file. In TUI mode, this CLI override takes precedence over the
refresh interval stored in the config file.

The app model's refresh interval is always clamped to the documented production
bounds: minimum `5` seconds, maximum `3600` seconds, default `30` seconds.
Binary-level TUI interval-refresh tests use the diagnostic-only
`AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS` hook as a runtime scheduling
override inside the test process. The hook can only shorten the event loop's
next-refresh timer below the already-clamped production interval; it cannot
lengthen the refresh interval, disable refreshes, or change the refresh interval
shown by the TUI/app model. It is not a supported user-facing configuration
option.

If no device URL is configured, `--tui` still opens the dashboard and shows the
missing-URL state instead of fetching. Set a URL with `config set-url`, or pass a
one-run URL override with `--url`:

```sh
airgradient-cli --tui --url 192.168.1.201
```

In TUI mode, `--url` takes precedence over the config-file `server_url` for that
run and the dashboard fetches only the override URL's `/measures/current`
endpoint. The override is not written to the config file.

When the TUI exits while a background fetch is pending, the runtime aborts the
fetch task and awaits the task handle before returning. Stale completions after
cancellation are ignored, and a fetch task panic is surfaced as a runtime error
when the task completion is observed.

PTY integration tests exercise the real `--tui` binary inside a pseudo-terminal
when the host platform can create one. On platforms or CI workers without usable
PTY support, those tests print a conditional-coverage skip reason and pass
without claiming full end-to-end terminal coverage. The runtime harness tests in
`tests/tui_runtime.rs` still cover TUI event-loop, fetch, shutdown, and cleanup
behavior in non-PTY environments. GitHub Actions also writes a test summary that
reports whether the PTY-backed coverage actually ran or was conditionally
skipped, so a green CI run does not hide the terminal coverage state. Expected
closed-PTY read errors are classified only through explicit target-specific
Unix `EIO` mappings for supported targets; raw OS error values from non-Unix or
unsupported targets are not treated as normal PTY closure.

## Config

The CLI reads and writes the same JSON config file as `airgradient-desktop`:

```text
$XDG_CONFIG_HOME/airgradient-desktop/config.json
```

If `XDG_CONFIG_HOME` is not set, it falls back to:

```text
$HOME/.config/airgradient-desktop/config.json
```

The known config fields are:

```json
{
  "server_url": "http://192.168.1.201/",
  "refresh_interval_secs": 30,
  "notifications_enabled": true,
  "start_minimized": false
}
```

`config set-url` and `config set-refresh` update only the known fields they own
and preserve unknown top-level sibling fields in the JSON file. This keeps the
shared config compatible with future `airgradient-desktop` fields.

The repair boundary is a top-level JSON object. If the config file contains a
top-level array, string, number, boolean, or `null`, the CLI reports an error
instead of trying to rewrite it. Preserving unknown sibling fields requires an
object to merge into, so non-object config JSON is not automatically repairable.

`config set-url` normalizes saved URLs:

- Bare hosts are accepted and saved as HTTP URLs, for example
  `192.168.1.201` becomes `http://192.168.1.201/`.
- `http` and `https` are the only supported schemes.
- Paths, queries, and fragments are stripped before saving the base URL.

`config set-refresh` accepts values from `5` to `3600` seconds. The default is
`30` seconds.

`config show` does not rewrite the config file. If a stored `server_url` is
missing or empty, it prints the rest of the effective config normally. If the
stored URL is malformed or uses an unsupported scheme, it prints the raw stored
value and writes a warning to stderr.

## Fetch Contract

One-shot fetches target the current measurements endpoint under the normalized
base URL:

```text
<server_url>/measures/current
```

For example, `http://192.168.1.201/` is fetched as:

```text
http://192.168.1.201/measures/current
```

Each one-shot fetch uses a 5 second HTTP timeout by default.
`AIRGRADIENT_CLI_FETCH_TIMEOUT_MS` can override that timeout in milliseconds,
but it is a diagnostic and test hook rather than a supported user-facing
configuration option. Normal users should rely on the default timeout.

Normal command stdout is reserved for command output. Text fetch output is
colored only when stdout is a terminal; captured or piped stdout has no ANSI
escapes by default. `--no-color` disables text color explicitly.

Default diagnostics are concise and uncolored:

```text
error: failed to request AirGradient measurements from http://192.168.1.201/measures/current
```

Use `-v` to include the error source chain:

```sh
airgradient-cli -v --url 192.168.1.201
```

Use `-vv` to include the source chain plus debug details and trace-level
diagnostics:

```sh
airgradient-cli -vv --url 192.168.1.201
```

## Sensor Parsing

The parser accepts current and alternate AirGradient field names, numeric JSON
values, numeric strings, and nested sensor payloads. Missing or invalid sensor
values stay missing, rendering as `--` in text output and `null` in JSON.

Sensor values are domain-checked before they reach presentation. The current
upper bounds are practical transport and firmware-glitch guardrails, not
calibrated hardware maximums: AQI `500`, CO2 `40000 ppm`, TVOC and NOx indexes
`500`, PM mass `1000 ug/m3`, PM0.3 count `1000000 / dL`, and temperature
`-40` to `85 C`. Humidity is limited to `0` through `100%`.
