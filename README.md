<div align="center">

<img src="site/assets/logo.svg" width="104" alt="airgradient-cli logo" />

# `airgradient-cli`

### 🌬️ Your air quality. In your terminal. No cloud, no browser, no nonsense.

A blazing-small **Rust** CLI + live **TUI dashboard** for your local AirGradient monitor.
Talks straight to the device on your LAN, shares the desktop app's config, and never phones home.

[![Release](https://img.shields.io/github/v/release/worxbend/airgradient-cli?style=for-the-badge&labelColor=0B1220&color=5AD6FF)](https://github.com/worxbend/airgradient-cli/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/worxbend/airgradient-cli/ci.yml?branch=main&style=for-the-badge&labelColor=0B1220&color=50FFA6&label=CI)](https://github.com/worxbend/airgradient-cli/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-D36EFF?style=for-the-badge&labelColor=0B1220)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.96.0-FFC75F?style=for-the-badge&labelColor=0B1220)](rust-toolchain.toml)
[![Platform](https://img.shields.io/badge/linux-amd64%20%C2%B7%20arm64-5AD6FF?style=for-the-badge&labelColor=0B1220)](#-install)

**[🌐 Website](https://worxbend.github.io/airgradient-cli/)** ·
**[📦 Releases](https://github.com/worxbend/airgradient-cli/releases)** ·
**[🎨 Themes](#-themes)** ·
**[⚙️ Config](#%EF%B8%8F-config)**

</div>

---

## ⚡ Install

```sh
curl --proto '=https' --tlsv1.2 -sSf \
  https://worxbend.github.io/airgradient-cli/install.sh | sh
```

That's it. The installer resolves the latest release, downloads the archive for
your architecture, **verifies its SHA256 checksum**, drops the binary in
`~/.local/bin`, and adds that directory to your `PATH` if it's missing.

<details>
<summary><b>Other ways to install</b></summary>

<br />

**Pin a specific version**

```sh
curl --proto '=https' --tlsv1.2 -sSf \
  https://github.com/worxbend/airgradient-cli/releases/download/v0.1.1/install.sh | sh -s -- --version v0.1.1
```

**Install somewhere else**

```sh
curl --proto '=https' --tlsv1.2 -sSf \
  https://worxbend.github.io/airgradient-cli/install.sh | sh -s -- --dir /usr/local/bin
```

**Read it before you run it** (always a good idea for a piped installer)

```sh
curl -fsSLO https://worxbend.github.io/airgradient-cli/install.sh
less install.sh
sh install.sh --help
```

**From source**

```sh
cargo install --path .
```

This installs the expected binary name, `airgradient-cli`, into Cargo's binary
directory.

| Installer option | Environment variable | Default |
| --- | --- | --- |
| `--version TAG` | `AIRGRADIENT_CLI_INSTALL_VERSION` | `latest` |
| `--dir DIR` | `AIRGRADIENT_CLI_INSTALL_DIR` | `$HOME/.local/bin` |

The same script is attached to every release as `install.sh` and served from the
website, so both URLs above fetch the identical file. It is a
download-and-verify convenience script, **not** a system package: it does not
register with a package manager, create a desktop entry, or install a service.
It refuses to run on any OS other than Linux, since only Linux binaries are
published.

</details>

> [!IMPORTANT]
> **Linux only.** Published binaries cover `x86_64-unknown-linux-gnu` (amd64) and
> `aarch64-unknown-linux-gnu` (arm64). On anything else, build from source.

---

## 🚀 60-Second Start

```sh
airgradient-cli config set-url 192.168.1.201   # 1. point it at your device
airgradient-cli                                # 2. read the air
airgradient-cli -t                             # 3. live dashboard
```

```text
Device: http://192.168.1.201/ | Fetch: 2ms
AQI 41 - Good

+-------------+-------+----------+----------+-------+
| Metric      | Value | Unit     | Status   | Trend |
+===================================================+
| AQI         | 41    |          | Good     | --    |
|-------------+-------+----------+----------+-------|
| CO2         | 612   | ppm      | Good     | --    |
|-------------+-------+----------+----------+-------|
| PM2.5       | 7.4   | ug/m3    | Good     | --    |
|-------------+-------+----------+----------+-------|
| PM0.3 count | 1234  | count/dL | Elevated | --    |
|-------------+-------+----------+----------+-------|
| TVOC        | 83    | index    | Good     | --    |
|-------------+-------+----------+----------+-------|
| Temperature | 22.6  | C        | Good     | --    |
+-------------+-------+----------+----------+-------+
```

---

## ✨ Features

|  | |
| --- | --- |
| 📟 **One-shot readings** | Run it bare, get an AQI headline plus the full metric table. Non-zero exit when the device is unreachable — drops straight into scripts. |
| 📊 **Live TUI dashboard** | `-t` opens an auto-refreshing Ratatui dashboard with gauges, trend arrows, and status colors mirroring the device's own LED. |
| 🧾 **JSON for pipelines** | `--json` emits every metric with raw value, formatted value, unit, status, and trend. Pipe it to `jq`, Prometheus, or cron. |
| 🎨 **20 built-in themes** | Nord, Dracula, Gruvbox, Catppuccin, Tokyo Night… plus a TTY-safe `mono` that leaves your terminal palette alone. |
| ⌨️ **Palette + config editor** | `:` for a command palette, `c` for a full-screen config form. Edits stay in a draft until you save. |
| 🔒 **Local-only, tolerant** | LAN only, nothing else. A malformed config warns per field instead of failing, so a bad value never locks you out of the tool that fixes it. |
| 🤝 **Shares desktop config** | Same JSON file as `airgradient-desktop`. Unknown fields are preserved on write. |

---

## 📖 Usage

### Commands

| Command | What it does |
| --- | --- |
| `airgradient-cli` | Fetch once and render the metric table |
| `airgradient-cli -t` / `--tui` | Open the live TUI dashboard |
| `airgradient-cli fetch` | Same one-shot request, explicitly |
| `airgradient-cli fetch --json` | Emit one JSON document of all metrics |
| `airgradient-cli themes` | List built-in theme ids and labels |
| `airgradient-cli config path` | Print the resolved config file path |
| `airgradient-cli config show` | Print the effective config as JSON |
| `airgradient-cli config set-url <URL>` | Save a device URL (bare hosts become `http://`) |
| `airgradient-cli config set-refresh <S>` | Save a refresh interval, `5`–`3600` seconds |
| `airgradient-cli config set-theme <ID>` | Save a TUI theme id |

### Flags

| Flag | Effect |
| --- | --- |
| `--url <URL>` | Override the device URL for this run only; never written to config |
| `--refresh <SECONDS>` | Override the TUI refresh interval for this run (`--tui` only) |
| `--theme <ID>` | Override the theme for this run |
| `--config <PATH>` | Use a different config file |
| `--json` | JSON instead of the table |
| `--no-color` | Strip ANSI color |
| `-v`, `-vv` | Add the error source chain; `-vv` adds debug + trace detail |

`--json` does not apply to config commands. `airgradient-cli --json config show`
exits with an error instead of silently changing the command output contract.

### Dashboard keys

| Key | Action |
| --- | --- |
| `r` | Refresh now |
| `+` / `-` | Lengthen / shorten the refresh interval |
| `:` | Command palette |
| `t` / `F2` | Theme picker with live preview |
| `c` | Config editor |
| `q` / `Esc` | Quit |

<details>
<summary><b>TUI behavior in detail</b></summary>

<br />

The TUI opens a Ratatui dashboard using the same AirGradient config file as the
desktop app. It fetches `<server_url>/measures/current` on startup when a device
URL is configured, then refreshes on the configured interval. If a later refresh
fails after a successful reading, the dashboard keeps showing the last successful
snapshot alongside the error.

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

On startup, the TUI shows a brief theme-colored splash screen before the
dashboard; any keypress skips it immediately (and, if that key was `q`/`Esc`,
also quits — no double keypress needed).

The status line at the top is a powerline-style bar with a colored dot for
the current AQI status (green/yellow/orange/red/purple, mirroring the
physical AirGradient device's LED), the app name, the device URL, and the
refresh interval. The powerline segment separators are drawn with a
Nerd-Font/powerline-patched glyph; other terminal fonts still render the
dashboard correctly, just without the smooth triangular joins. Metric cards
also show a thin gauge bar under each reading, filled relative to that
metric's typical scale and colored by its status.

If no device URL is configured, `--tui` still opens the dashboard and shows the
missing-URL state instead of fetching. Set a URL with `config set-url`, or pass a
one-run URL override with `--url`:

```sh
airgradient-cli --tui --url 192.168.1.201
```

In TUI mode, `--url` takes precedence over the config-file `server_url` for that
run and the dashboard fetches only the override URL's `/measures/current`
endpoint. The override is not written to the config file.

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
shown by the TUI/app model. Values below `100` milliseconds, zero, invalid
values, values equal to the production interval, and values longer than the
production-clamped interval are ignored. It is not a supported user-facing
configuration option.

When the TUI exits while a background fetch is pending, the runtime aborts the
fetch task and awaits the task handle before returning. Stale completions after
cancellation are ignored, and a fetch task panic is surfaced as a runtime error
when the task completion is observed.

</details>

---

## 🎨 Themes

```sh
airgradient-cli themes                  # list all 20
airgradient-cli config set-theme nord   # persist one
airgradient-cli --tui --theme nord      # just this run
```

`default` · `claude` · `codex` · `btop` · `nord` · `dracula` · `gruvbox` ·
`solarized-dark` · `monokai` · `one-dark` · `tokyo-night` · `catppuccin-mocha` ·
`rose-pine` · `ayu-dark` · `everforest-dark` · `kanagawa` · `synthwave-84` ·
`github-dark` · `nightfox` · `mono`

An unrecognized theme id never errors — it silently falls back to the `default`
theme, both when reading the config file and with `--theme`.

Inside the TUI, press `t` (or `F2`) to open the theme picker: `↑`/`↓` (or
`j`/`k`) live-preview a theme across the whole dashboard, `Enter` applies and
persists it to the config file, and `Esc` (or `q`/`F2`) reverts to whatever
was active before the picker opened.

### Command palette

Press `:` to open the command palette, a single-line prompt at the bottom of
the dashboard. It accepts:

| Command | Effect |
| --- | --- |
| `url <URL>` | Sets the device URL, applied immediately and persisted |
| `refresh <SECONDS>` | Sets the refresh interval (`5`-`3600`), applied and persisted |
| `theme <ID>` | Sets the theme, applied and persisted |
| `config` / `settings` | Opens the full config editor |
| `themes` | Opens the theme picker |
| `save` | Confirms the current in-memory config is persisted |
| `quit` / `q` | Quits the TUI |

`Esc` cancels the palette; `Backspace` edits the typed line.

Press `c` to open the full config editor: a page listing every config field
(server URL, refresh interval, notifications, start-minimized, theme).
`↑`/`↓` navigate, `Enter` starts editing a text field (or toggles a boolean,
or opens the theme picker for the theme row), and a trailing "Save & Close"
row writes every field to the config file at once. `Esc` cancels the field
currently being edited, or discards the whole draft and returns to the
dashboard if pressed while just navigating.

---

## ⚙️ Config

The CLI reads and writes the same JSON config file as `airgradient-desktop`:

```text
$XDG_CONFIG_HOME/airgradient-desktop/config.json
```

If `XDG_CONFIG_HOME` is not set, it falls back to:

```text
$HOME/.config/airgradient-desktop/config.json
```

```json
{
  "server_url": "http://192.168.1.201/",
  "refresh_interval_secs": 30,
  "notifications_enabled": true,
  "start_minimized": false,
  "theme": "default"
}
```

<details>
<summary><b>Config rules and repair behavior</b></summary>

<br />

`config set-url`, `config set-refresh`, and `config set-theme` update only the
known field they own and preserve unknown top-level sibling fields in the JSON
file. This keeps the shared config compatible with future `airgradient-desktop`
fields. `theme` accepts any string; see `airgradient-cli themes` for the list
of built-in ids, and note that an unrecognized id resolves to the `default`
theme rather than erroring.

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

</details>

---

## 🔌 Fetch Contract

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

Use `-v` to include the error source chain, and `-vv` to add debug details and
trace-level diagnostics:

```sh
airgradient-cli -v --url 192.168.1.201
airgradient-cli -vv --url 192.168.1.201
```

---

## 🔬 Sensor Parsing

The parser accepts current and alternate AirGradient field names, numeric JSON
values, numeric strings, and nested sensor payloads. Missing or invalid sensor
values stay missing, rendering as `--` in text output and `null` in JSON.

Sensor values are domain-checked before they reach presentation. The current
upper bounds are practical transport and firmware-glitch guardrails, not
calibrated hardware maximums:

| Metric | Bounds |
| --- | --- |
| AQI | `500` |
| CO2 | `40000 ppm` |
| TVOC / NOx index | `500` |
| PM mass | `1000 ug/m3` |
| PM0.3 count | `1000000 / dL` |
| Temperature | `-40` to `85 C` |
| Humidity | `0` to `100%` |

---

## 🛠️ Development

```sh
cargo test                                            # 238 tests
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt
```

Code conventions are written down in **[`docs/code-style.md`](docs/code-style.md)**.
The microsite lives in [`site/`](site/) and deploys to GitHub Pages on push to
`main`.

---

## 📦 Releases

Releases are tag-driven. Pushing a `vX.Y.Z` tag that matches the `Cargo.toml`
version triggers `.github/workflows/release.yml`, which builds the Linux amd64
and arm64 artifacts, creates or reuses the GitHub release, and uploads the
archives, `SHA256SUMS`, and `install.sh`. It does not tag commits, sign
artifacts, publish crates.io packages, or create package-manager recipes.
`Cargo.toml` intentionally keeps `publish = false`, so the package is not
intended for crates.io publishing yet.

The repository license is MIT. Every release artifact bundle or release
attachment set must include the checked-in `LICENSE` file.

<details>
<summary><b>Release rehearsal and validation gate</b></summary>

<br />

Rehearse the Linux release artifact locally before publishing anything:

```sh
scripts/release-dry-run.sh --target x86_64-unknown-linux-gnu --output-dir dist
scripts/release-dry-run.sh --target aarch64-unknown-linux-gnu --output-dir dist-arm64
```

The supported release dry-run targets are `x86_64-unknown-linux-gnu` (amd64)
and `aarch64-unknown-linux-gnu` (arm64). The dry run rejects every other target
before building, staging, or writing artifacts, including when `--skip-build` is
used. Use a new or empty staging directory for local rehearsal so stale files
cannot be mistaken for the current release. CI may use a temporary output
directory for the same validation-only check. The dry run reads the crate
version from `Cargo.toml`, builds with Cargo, and stages outputs without
tagging, creating a GitHub release, uploading, signing, publishing, generating
shell completions, or producing package-manager recipes, macOS binaries, or
Windows binaries. Expected outputs are:

- `dist/airgradient-cli-v<version>-x86_64-unknown-linux-gnu.tar.gz`
- `dist-arm64/airgradient-cli-v<version>-aarch64-unknown-linux-gnu.tar.gz`
- `dist/SHA256SUMS`

The `.tar.gz` bundle must contain the built `airgradient-cli` executable and
the checked-in `LICENSE`. `SHA256SUMS` is generated over the staged release
artifact file. Detached cryptographic signatures are intentionally out of scope;
releases must not describe artifacts as signed.

The repository does not currently generate or package shell completions, so
release artifacts and release notes should not list completions unless
completion generation support is added and tested first. The maintainer release
checklist is in `docs/release-checklist.md`, and the release boundary audit is
in `docs/release-boundary.md`.

**Validation gate.** Release validation is pinned to Rust 1.96.0 and cargo-deny
0.19.9. Maintainers should run the local checks in the same order as CI:

```sh
cargo deny check
scripts/release-dry-run.sh --target x86_64-unknown-linux-gnu --output-dir dist
scripts/release-dry-run.sh --target aarch64-unknown-linux-gnu --output-dir dist-arm64
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

After `cargo test`, record the PTY coverage summary: real pseudo-terminal
coverage exercised, PTY unavailable and conditionally skipped, or infrastructure
failure. Also record real-device validation status; if no real AirGradient
hardware run was performed, release notes must explicitly waive that validation
gap.

</details>

<details>
<summary><b>Dependency policy</b></summary>

<br />

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

Tool pins are part of the release contract. Keep `README.md`,
`rust-toolchain.toml`, `.github/workflows/ci.yml`, and release notes
synchronized when changing Rust 1.96.0 or cargo-deny 0.19.9, and run the full
release validation suite after a pin update because rustfmt, Clippy, and
cargo-deny behavior can change. Periodically rerun
`cargo tree -d --target all` and prune exact duplicate-version exceptions from
`deny.toml` when upstream dependencies converge.

</details>

<details>
<summary><b>Terminal test coverage</b></summary>

<br />

PTY integration tests exercise the real `--tui` binary inside a pseudo-terminal
when the host platform can create one. On platforms or CI workers without usable
PTY support, those tests print a conditional-coverage skip reason and pass
without claiming full end-to-end terminal coverage. The runtime harness tests in
`src/tui/runtime/tests/` still cover TUI event-loop, fetch, shutdown, and cleanup
behavior in non-PTY environments. GitHub Actions also writes a test summary that
reports whether the PTY-backed coverage actually ran or was conditionally
skipped, so a green CI run does not hide the terminal coverage state. Expected
closed-PTY read errors are classified only through platform-provided
`libc::EIO` on supported Unix-like targets; raw OS error values from non-Unix or
unsupported targets are not treated as normal PTY closure.

</details>

---

<div align="center">

**MIT licensed** · Built with 🦀 in Rust · [worxbend/airgradient-cli](https://github.com/worxbend/airgradient-cli)

</div>
