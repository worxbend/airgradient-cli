# Usage guide

Detailed reference for airgradient-cli v0.4.1. [Quick start](../README.md) · [Release notes](releases/v0.4.1.md)

- [Installation](#installation)
- [Commands and flags](#commands-and-flags)
- [Keyboard and mouse](#keyboard-and-mouse)
- [Dashboard behavior](#dashboard-behavior)
- [Themes and appearance](#themes-and-appearance)
- [Command palette and settings](#command-palette-and-settings)
- [Configuration](#configuration)
- [Alerts](#alerts)
- [History](#history)
- [Fetch contract](#fetch-contract)
- [Sensor parsing](#sensor-parsing)
- [Development and release policy](#development-and-release-policy)

## Installation

Published binaries support Linux amd64 (`x86_64-unknown-linux-gnu`) and arm64
(`aarch64-unknown-linux-gnu`). The installer refuses other operating systems or
architectures. Source builds are the alternative, not a promise of platform support.

### Latest release

```sh
curl --proto '=https' --tlsv1.2 -sSf \
  https://worxbend.github.io/airgradient-cli/install.sh | sh
```

This executes a downloaded script. To inspect it first:

```sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://worxbend.github.io/airgradient-cli/install.sh -o install.sh
less install.sh
sh install.sh --help
sh install.sh
```

### Pinned release

```sh
curl --proto '=https' --tlsv1.2 -sSfL \
  https://github.com/worxbend/airgradient-cli/releases/download/v0.4.1/install.sh \
  | sh -s -- --version v0.4.1
```

### Custom directory

```sh
sh install.sh --version v0.4.1 --dir /usr/local/bin
```

The directory must be writable by the current user.

| Installer option | Environment variable | Default |
| --- | --- | --- |
| `--version TAG` | `AIRGRADIENT_CLI_INSTALL_VERSION` | `latest` |
| `--dir DIR` | `AIRGRADIENT_CLI_INSTALL_DIR` | `$HOME/.local/bin` |

The installer resolves the release, downloads the matching architecture archive,
verifies its SHA256 checksum, and installs `airgradient-cli`. If the destination
is absent from `PATH`, it adds entries to `~/.bashrc` and `~/.zshrc`. Restart your
shell, or update the current shell's path yourself.

The script is served from the website and attached to releases as `install.sh`.
A pinned release script stays tied to that release; the website copy can change.
It is a download-and-verify helper, not a system package. It creates no desktop
entry, service, or package-manager registration. Checksums are not signatures;
release artifacts are unsigned.

### From source

With Git and the pinned Rust toolchain available:

```sh
git clone https://github.com/worxbend/airgradient-cli.git
cd airgradient-cli
git checkout v0.4.1
cargo install --locked --path .
```

The executable is named `airgradient-cli` and is installed in Cargo's binary directory.
The crate has `publish = false`; this is not a crates.io installation.

## Commands and flags

Set a device address, then fetch once or open the dashboard:

```sh
airgradient-cli config set-url 192.168.1.201
airgradient-cli
airgradient-cli -t
```

Replace the example IP with your LAN device address.

| Command | Effect |
| --- | --- |
| `airgradient-cli` | Fetch once; print AQI and a metric table |
| `airgradient-cli -t` / `--tui` | Open the live dashboard |
| `airgradient-cli fetch` | Explicit one-shot fetch |
| `airgradient-cli fetch --json` | Emit one JSON document of all metrics |
| `airgradient-cli themes` | List built-in theme ids and labels |
| `airgradient-cli config path` | Print the resolved config path |
| `airgradient-cli config show` | Print effective config as JSON |
| `airgradient-cli config set-url <URL>` | Save a device base URL |
| `airgradient-cli config set-refresh <S>` | Save an interval, 5–3600 seconds |
| `airgradient-cli config set-theme <ID>` | Save a TUI theme id |

| Flag | Effect |
| --- | --- |
| `--url <URL>` | Device URL for this run; does not save it |
| `--refresh <SECONDS>` | TUI interval for this run; does not save it |
| `--theme <ID>` | TUI theme for this run; does not save it |
| `--config <PATH>` | Use a different config file |
| `--json` | JSON instead of the one-shot table |
| `--no-color` | Disable one-shot text color |
| `-v`, `-vv` | Error source chain; debug and trace detail at `-vv` |
| `--help`, `--version` | Print help or version |

Put top-level options before subcommands, for example:
`airgradient-cli --url 192.168.1.201 fetch --json`.

`--tui` cannot be combined with a subcommand or `--json`.
`--refresh` is rejected outside TUI mode. `--json` is rejected for `config` and
`themes`; `config show` already emits JSON without that flag.

One-shot trends compare against the previously recorded reading. The first run,
recording opt-out, or an unavailable previous reading leaves the trend unknown
(`--` in text). JSON includes raw and formatted values, units, status, and trend.

## Keyboard and mouse

Bindings follow Vim and [AstroNvim](https://astronvim.com) conventions.
`Space` opens a which-key popup showing the next key's action.

### Dashboard

| Key | Action |
| --- | --- |
| `Space` | Leader / key hints |
| `Space` then `r` / `t` / `c` / `h` / `:` / `q` | Refresh / themes / config / history / palette / quit |
| `r` | Refresh now |
| `+` / `-` | Lengthen / shorten refresh interval |
| `:` | Command palette |
| `t` / `F2` | Theme picker with live preview |
| `c` | Config editor |
| `h` | History chart |
| `q` / `Esc` / `Ctrl-c` | Quit |

### Lists: themes, config, history

| Key | Action |
| --- | --- |
| `j` / `k`, `Down` / `Up` | Move down / up |
| `gg` / `G` | First / last item |
| `Ctrl-d` / `Ctrl-u` | Half-page down / up |
| `Enter` / `l` / `Right` | Apply, edit, or toggle |
| `Esc` / `q` / `h` / `Ctrl-c` | Close without applying |

In history, the motions select which metric is plotted.

### Text entry: palette and config fields

| Key | Action |
| --- | --- |
| `Ctrl-w` | Delete the word before the cursor |
| `Ctrl-u` | Clear the line |
| `Backspace` | Delete a character |
| `Enter` | Submit |
| `Esc` / `Ctrl-c` | Cancel |

### Mouse

Scroll to move through theme/config lists; left-click to select a row.
Mouse reporting replaces normal terminal text selection. Hold `Shift` in most
terminals to select text. Mouse capture is best-effort; keyboard navigation
still works when a terminal refuses it.

## Dashboard behavior

The Ratatui dashboard fetches `<server_url>/measures/current` at startup, then at
the configured interval. A failed refresh retains the last successful snapshot
and displays the error; the retained reading is not a fresh measurement.

The TUI needs an interactive terminal. Captured or piped terminal streams cause
`TUI requires an interactive terminal`. Minimum dashboard size is 36 columns by
20 rows. Smaller windows show a resize message. At 36×20, priority AQI, status,
and controls remain visible, but lower metric rows may be clipped. There is no
metric-card scrolling or pagination at that compact size.

A brief theme-colored splash appears on startup. Any key skips it; `q` or `Esc`
also quits without requiring another press. The TUI restores terminal state on
exit and runtime-error paths after setup starts: alternate screen, cursor, and raw mode.

Without a configured URL, the dashboard opens in a missing-URL state rather
than fetching. Supply `--url` or save an address with `config set-url`.

```sh
airgradient-cli --tui --url 192.168.1.201 --refresh 10
```

Run-only URL and interval overrides take precedence over config-file values.
Only the override URL's `/measures/current` endpoint is fetched; the override
is not written to config. Refresh bounds are 5–3600 seconds; default is 30.
Explicit edits through the palette or settings are separate, saved actions.

## Themes and appearance

```sh
airgradient-cli themes
airgradient-cli config set-theme nord
airgradient-cli --tui --theme nord
```

Built-in ids:

`default` · `claude` · `codex` · `btop` · `nord` · `dracula` · `gruvbox` ·
`solarized-dark` · `monokai` · `one-dark` · `tokyo-night` · `catppuccin-mocha` ·
`rose-pine` · `ayu-dark` · `everforest-dark` · `kanagawa` · `synthwave-84` ·
`github-dark` · `nightfox` · `mono`

Unknown theme ids fall back to `default`, whether loaded from config or passed
with `--theme`. They do not cause an error.

Press `t` or `F2` to open the picker. Arrow keys or `j`/`k` preview the theme
across the dashboard. `Enter` applies and saves it. `Esc`, `q`, or `F2` reverts
to the theme active before opening the picker.

Status colors are shared across badges, cards, gauges, and chart lines:
green (Good), yellow (Moderate), orange (Elevated), red (Unhealthy), and magenta
(Very unhealthy). Labels carry meaning alongside color.

Gauges show colored bands across their scale, with the reading marked by the
solid portion. Tick marks and scale labels appear when space permits. Wide
layouts use separated panels; compact layouts give space back to readings.

Icons use the Material Design range from [Nerd Fonts](https://www.nerdfonts.com/).
Powerline separators also need a compatible font. If glyphs become empty boxes,
use `mono`: plain-Unicode icons, no Nerd Font dependency, and terminal-default
backgrounds. Every icon has a label; no meaning depends on the icon alone.

Spinners and gradients advance during activity such as fetching, alerts, and
the splash. An idle dashboard is still rather than continuously repainted.
See [TUI design](tui-design.md) for component contracts and synthetic renderer previews.

## Command palette and settings

Press `:` for the floating command dialog.

| Command | Effect |
| --- | --- |
| `url <URL>` | Apply and save device URL |
| `refresh <SECONDS>` | Apply and save interval, 5–3600 seconds |
| `theme <ID>` | Apply and save theme |
| `config` / `settings` | Open config editor |
| `themes` | Open theme picker |
| `save` | Persist current in-memory config |
| `quit` / `q` | Quit the TUI |

`Esc` cancels; `Backspace` edits the line.

Press `c` for the full settings editor: server URL, refresh interval,
notifications, start-minimized, and theme. Arrow keys navigate. `Enter` edits a
text field, toggles a boolean, or opens the theme picker. The final **Save & Close**
row writes the draft. `Esc` cancels the current field edit; while navigating,
it discards the draft and returns to the dashboard.

## Configuration

The CLI shares JSON config with `airgradient-desktop`:

```text
$XDG_CONFIG_HOME/airgradient-desktop/config.json
```

Without `XDG_CONFIG_HOME`, it uses `$HOME/.config/airgradient-desktop/config.json`.
`--config <PATH>` overrides the location.

```json
{
  "server_url": "http://192.168.1.201/",
  "refresh_interval_secs": 30,
  "notifications_enabled": true,
  "start_minimized": false,
  "theme": "default"
}
```

`config set-url`, `config set-refresh`, and `config set-theme` update their owned
field and preserve unknown top-level sibling fields. This keeps future desktop
settings intact. Invalid known fields produce warnings rather than silently
blocking access to the commands that repair them.

The repair boundary is a top-level JSON object. Arrays, strings, numbers,
booleans, and `null` are not automatically repairable: setters report an error
because there is no object into which unknown sibling fields can be merged.

Saved URLs are normalized:

- Bare hosts become HTTP URLs: `192.168.1.201` becomes `http://192.168.1.201/`.
- Only `http` and `https` are supported.
- Paths, queries, and fragments are removed from the saved base URL.

Saved intervals accept 5–3600 seconds; default is 30. Theme accepts any string,
with unknown ids rendering as `default`.

`config show` never rewrites the file. Missing or empty `server_url` does not
prevent displaying the remaining effective config. Malformed or unsupported
stored URLs are printed as stored, with a warning on stderr.

## Alerts

TUI toasts report meaningful changes, not every concerning reading:

- Entering Elevated or worse alerts. Worsening within concerning bands alerts again.
- Moderate is below the alert threshold.
- The first reading never alerts: there is no previous reading to compare.
- Transitions to or from Unknown are ignored. A missing sensor is not bad air.
- Recovery is announced too.

Multiple alerts queue worst-first. Each gets six seconds of display time.

## History

Successful one-shot fetches and dashboard refreshes record readings. The chart
(`h`) loads saved history at startup, so data can survive restarts. A cron or
`watch` loop using one-shot fetches builds the same history.

| Property | Value |
| --- | --- |
| Path | `$XDG_DATA_HOME/airgradient-cli/history.jsonl` |
| Fallback | `$HOME/.local/share/airgradient-cli/history.jsonl` |
| Format | [JSON Lines](https://jsonlines.org/): one `{"at_unix_secs":…, "snapshot":{…}}` per line, oldest first |
| Retention | Most recent 2000 readings |
| Opt-out | Set `AIRGRADIENT_CLI_NO_HISTORY` to any value to disable recording |

History belongs in the data directory, not the shared desktop config directory.
Recording is best-effort: a write failure does not make a successful fetch fail.
The recording opt-out covers the dashboard and one-shot fetches, but still lets
the dashboard read existing history. It does not block explicit config writes.

```sh
AIRGRADIENT_CLI_NO_HISTORY=1 airgradient-cli --json

# Last 20 CO2 readings; honor XDG_DATA_HOME when set.
tail -20 "${XDG_DATA_HOME:-$HOME/.local/share}/airgradient-cli/history.jsonl" \
  | jq -r '[.at_unix_secs, .snapshot.co2] | @tsv'
```

## Fetch contract

Requests use the normalized device base URL plus `/measures/current`:

```text
http://192.168.1.201/measures/current
```

The default HTTP timeout is 5 seconds. Failed fetches return a non-zero exit.
Normal stdout is reserved for command output. One-shot text is colored only
when stdout is a terminal; captured or piped text contains no ANSI escapes by
default. `--no-color` explicitly disables text color.

Default diagnostics are concise and uncolored:

```text
error: failed to request AirGradient measurements from http://192.168.1.201/measures/current
```

```sh
airgradient-cli -v --url 192.168.1.201    # Error source chain.
airgradient-cli -vv --url 192.168.1.201   # Debug and trace diagnostics.
```

`AIRGRADIENT_CLI_FETCH_TIMEOUT_MS` overrides the timeout in milliseconds.
It is a diagnostic/test hook, not a supported user setting. Use the default
for normal operation.

## Sensor parsing

The parser accepts current and alternate field names, numeric JSON values,
numeric strings, and nested sensor payloads. Missing or invalid values remain
missing: `--` in text, `null` in JSON. They are not replaced with zero.

Compensated values follow the firmware's `monitorDisplayCompensatedValues`
switch so readings align with the device's display choice.

Domain checks reject implausible values before presentation. Upper bounds are
transport and firmware-glitch guardrails, not calibrated hardware maximums.

| Metric | Bounds |
| --- | --- |
| AQI | Upper bound `500` |
| CO2 | Upper bound `40000 ppm` |
| TVOC / NOx index | Upper bound `500` |
| PM mass | Upper bound `1000 ug/m3` |
| PM0.3 count | Upper bound `1000000 / dL` |
| Temperature | `-40` to `85 C` |
| Humidity | `0` to `100%` |

## Development and release policy

See [code style](code-style.md), [TUI design](tui-design.md),
[release checklist](release-checklist.md), and [release boundary](release-boundary.md).
The microsite lives in [`site/`](../site/) and deploys to GitHub Pages on pushes to `main`.

### Gate and distribution

Release validation pins **Rust 1.96.0** and **cargo-deny 0.19.9**. Run in CI order:

```sh
cargo deny check
scripts/release-dry-run.sh --target x86_64-unknown-linux-gnu --output-dir dist
scripts/release-dry-run.sh --target aarch64-unknown-linux-gnu --output-dir dist-arm64
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Run these from the repository root. Use `cargo fmt` to apply formatting.
For doc-only release-contract changes, the focused check is
`cargo test --locked --test release_contract_docs`.

A `vX.Y.Z` tag matching `Cargo.toml` triggers
[the release workflow](../.github/workflows/release.yml), which builds Linux
amd64/arm64, creates or reuses the GitHub release, and uploads archives,
`SHA256SUMS`, and `install.sh`. It does not create the tag, sign artifacts,
publish to crates.io, or create package-manager recipes. `publish = false` is intentional.

The dry run is validation-only. Use a new or empty output directory to avoid
mistaking old files for current artifacts. It reads the crate version, builds,
and stages bundles. Every other target is rejected before building or staging,
even with `--skip-build`. CI may use a temporary output directory.

Local staged names contain the target triple:

- `airgradient-cli-v<version>-x86_64-unknown-linux-gnu.tar.gz`
- `airgradient-cli-v<version>-aarch64-unknown-linux-gnu.tar.gz`
- `SHA256SUMS` covering the staged archive in each directory.

Published archive names use `linux-amd64` and `linux-arm64` instead. Bundles
contain the executable and checked-in MIT `LICENSE`; every release bundle or
attachment set must include that license. No detached signatures, shell
completions, package-manager recipes, or macOS/Windows binaries are produced.
Do not claim these exist without adding and testing support.

### Dependency policy

- Vulnerabilities and yanked crates block release unless patched, replaced, or handled with a documented reason.
- Collapse duplicate versions where reasonable. Otherwise exempt only the specific package/version and explain why.
- Confirm license compatibility before extending the allowlist.
- Review unknown registries and Git sources before allowing them.

Keep pins synchronized in [`README.md`](../README.md),
[`rust-toolchain.toml`](../rust-toolchain.toml), [CI](../.github/workflows/ci.yml),
and release notes. Run the full gate after pin updates: formatter, Clippy, and
cargo-deny behavior can change. Periodically run `cargo tree -d --target all`
and prune exact duplicate-version exceptions from `deny.toml` as dependencies converge.

### Terminal and hardware validation

After `cargo test`, record PTY coverage: real pseudo-terminal coverage exercised,
PTY unavailable and conditionally skipped, or infrastructure failure.
Record hardware validation separately; release notes must explicitly waive a
missing real-device run. Synthetic fixtures and mock HTTP devices do not prove hardware compatibility.

PTY integration tests run the real `--tui` binary when the host can create a
pseudo-terminal. Otherwise they report a conditional-coverage skip and pass
without claiming end-to-end terminal coverage. CI also reports that state.
[`src/tui/runtime/tests/`](../src/tui/runtime/tests/) still covers event-loop,
fetch, shutdown, and cleanup behavior without PTYs. Expected closed-PTY reads
are classified through platform-provided `libc::EIO` on supported Unix-like
targets, never guessed raw error numbers from unsupported targets.

`AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS` is a diagnostic-only scheduler
hook for binary-level interval tests, not a user setting. It can only shorten
the next-refresh timer below the already-clamped production interval. It cannot
lengthen or disable refreshes, change the model interval, or change its display.
Values below 100 milliseconds, zero, invalid values, and values equal to or
longer than the production interval are ignored. The model always keeps the
5–3600-second bounds and 30-second default.

On exit, the runtime aborts a pending background fetch and awaits its task handle.
Stale completions after cancellation are ignored. A fetch-task panic becomes a
runtime error when its completion is observed.
