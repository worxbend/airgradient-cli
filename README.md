# AirGradient CLI

`airgradient-cli` is a command-line companion for an AirGradient device. By
default it reads the shared desktop config, fetches the current measurements
once, and prints compact terminal output.

The interactive TUI dashboard is available with `-t` / `--tui` in an
interactive terminal.

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

Override the TUI refresh interval for one run:

```sh
airgradient-cli --tui --refresh 10
```

`--refresh <SECONDS>` applies only to `--tui` and is rejected for one-shot
fetches and config commands. Like `config set-refresh`, it accepts values from
`5` to `3600` seconds. It changes only the current dashboard run and does not
write the config file.

If no device URL is configured, `--tui` still opens the dashboard and shows the
missing-URL state instead of fetching. Set a URL with `config set-url`, or pass a
one-run URL override with `--url`:

```sh
airgradient-cli --tui --url 192.168.1.201
```

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
