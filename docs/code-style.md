# Code Style

Conventions this repository actually follows, written down after the
2026-08-01 refactor so the next change matches the code around it. These are
the project-specific rules; the general reasoning behind them is in
`GUIDELINES.md` at the repo root.

## Module Size and Shape

- **One responsibility per file.** A file that renders a dashboard does not
  also format durations; a file that owns the event loop does not also own
  terminal teardown.
- **Target 200-300 lines, hard-limit 500.** When a file crosses that, the fix
  is to find the seam, not to compress the code. `src/tui/ui/` and
  `src/tui/runtime/` show the intended shape: a small `mod.rs` that dispatches,
  and siblings that each own one surface.
- The limit applies to every file under `src/`, tests included. Two test files
  had drifted past it and were split by subject (`src/tui/app/tests/`,
  `src/tui/runtime/tests/input_routing.rs`); a claim about the exact current
  maximum is deliberately not repeated here, because it goes stale the moment
  someone adds a function. Check it instead:

  ```sh
  find src -name '*.rs' | xargs wc -l | sort -rn | head
  ```

- One exception worth naming: `src/tui/theme/palette.rs` is a data table — one
  constant per theme — and reads top to bottom without holding any state in
  mind. Prefer keeping a table whole over splitting it at an arbitrary
  midpoint.
- Test doubles get the same treatment as production code: the runtime's fakes
  are split by which seam they stand in for (`terminal_fakes`, `fetch_fakes`)
  rather than piled into the test module's `mod.rs`.

## Module Layout

```
src/<area>/mod.rs      types, public API, dispatch — what a reader hits first
src/<area>/<thing>.rs  one responsibility, private to the area unless re-exported
src/<area>/tests.rs    or tests/ for a multi-file suite
```

- Prefer `pub(super)` between siblings; reserve `pub` for the crate's real API.
- Re-export the public surface from `mod.rs` (`pub use store::{read_config, …}`)
  so splitting a module never changes a caller's import path.
- Inline `#[cfg(test)] mod tests` is fine while a file is small; move it to a
  sibling `tests.rs` once the file approaches the size limit.

## Comments and Docs

- **Every module gets a `//!` header** stating what it owns and, where it is
  not obvious, what it deliberately does *not* own.
- **Document the why, not the what.** A comment earns its place by recording a
  decision, a constraint, or a trap:
  - why only one fetch is ever in flight (`src/tui/runtime/fetch.rs`)
  - why cleanup steps run in that exact order (`src/tui/runtime/terminal.rs`)
  - why a malformed config warns instead of failing (`src/config/lossy.rs`)
- Do not caption obvious code. `// increment i` is worse than no comment.
- Non-obvious constants get a doc comment explaining the number's origin —
  a bare `112` in a layout is a mystery six months later.

## Types That Cannot Be Wrong

Prefer a type that makes an invalid value unrepresentable over a check every
caller has to remember. Two of these carry the domain rules:

- `DeviceUrl` — an http/https address with no credentials and no path. Built
  only by `DeviceUrl::parse`, so holding one is proof it was validated.
- `RefreshInterval` — 5..=3600 seconds. Two constructors, and choosing between
  them is the decision: `from_secs` rejects (use it for anything a person
  typed), `clamping` pulls into range (use it only for the program's own
  values, like the `+`/`-` keys walking off the end).

Both sit at the crate root, below every module that uses them. That is the
point: `config` stores a device address and `device` fetches from one, and
before the type existed `config` had to import `device` to borrow its
validation — which put the HTTP client underneath the config file reader. A
rule shared by two modules belongs beneath both, not inside one of them.

The test to apply: if you are about to write the same validation at a fifth
call site, or to write `.expect()` about a guarantee something else made, the
guarantee wants to be a type. `Metrics::aqi` is the same idea in the small — it
returns `&Metric` rather than `Option`, so three renderers stopped asserting
that an AQI row exists.

Keep the persistence shape separate from the domain shape when they genuinely
differ. `Config::server_url` is still a `String`, because the file may hold a
value the domain rejects and `config show` has to display exactly that so the
user can see what needs fixing.

## Errors

- Errors carry context: the path, the value, the expected range. Compare
  `refresh_interval::OutOfRange`, which names the bounds *and* what it got.
- Wrap an error only to add context the reader does not already have. A
  wrapper like "failed to normalize server URL" around "device URL must not
  embed credentials" buries the actionable half behind `-v`; that one is
  `#[error(transparent)]` now.
- Never let a cleanup failure hide the failure that caused it — see
  `RuntimeError::Cleanup` / `RuntimeError::Secondary`.
- Tolerate bad input where the user needs the tool to fix it. A broken config
  file must not block the command that repairs the config file.

## Control Flow

- Guard clauses and `let … else` over nesting. Two levels is the norm; four is
  a refactor.
- Extract a named function when a loop body needs a comment to explain what
  the block does — `refresh_if_due` and `apply_view_event` came out of the
  event loop that way.
- Do not repeat a multi-step sequence at more than one call site. The
  "request a fetch and drain results" block appeared four times in the loop
  before it became `LoopContext::start_refresh_if_configured`.
- Treat a repeated `#[allow(clippy::too_many_arguments)]` as a missing type.
  Three functions in the event loop threaded the same six values through their
  signatures; they are `LoopContext`'s fields now, and the loop's steps are
  methods.
- Dispatch on the thing that changes. `apply_view_event` used to match on the
  event with a nested `match app.view` inside eleven arms, so adding a view
  meant editing all eleven and missing one failed silently. It matches on the
  view now, with one handler per view listing only the events that view binds.

## Shared Values

Presentation constants belong to the layer that owns the concept, not to each
renderer. `MISSING_VALUE` and `Trend::display_symbol` live in
`src/sensors/presentation.rs` because the text, JSON, and TUI outputs must all
show the same placeholder for the same absent reading.

Where two layers genuinely need different behavior, give the functions
different names and say why in a doc comment. `format_fetch_latency` (precise,
for scraped CLI output) and `ui::format::format_duration` (coarse, for a
status bar) are deliberately separate — but they must not both be called
`format_duration`, because then grepping the name cannot tell you which one a
call site meant.

## The Shared Config File

`~/.config/airgradient-desktop/config.json` belongs to the AirGradient desktop
app as much as to this CLI, which constrains every write:

- **Merge, never replace.** `write_config` writes into the existing JSON object
  so keys this CLI does not model survive.
- **Replace atomically.** New content goes to a sibling temp file, is
  `sync_all`'d, then `rename`d, so a crash mid-write cannot leave a truncated
  file that breaks both programs.
- **Do not silently win a race.** A read-modify-write whose file changed
  underneath it is refused (`ConfigError::ConcurrentModification`) rather than
  overwriting the other program's change. It is a narrow guard, not a lock —
  the doc comment on `update_config` is explicit about what it does and does
  not cover. The rule has no exemption for slow writers: the TUI's config
  editor holds its draft for as long as the user is typing, which is the
  longest such window in the program, so it takes a `config_version` before
  reading and saves through `write_config_if_unchanged`. Reach for bare
  `write_config` only where the content owes nothing to a prior read.
- **Write only what the command is about.** `:save` in the TUI leaves
  `server_url` alone when the session has no URL, because "save what I have"
  is not "clear what you have".

## Derived Data

The reading history (`src/history.rs`) is a different kind of file from the
config, and follows different rules:

- **Append, do not rewrite.** One JSON object per line, so recording a reading
  is a single `write` with nothing to parse first. A JSON array would mean
  re-serializing the whole history on every refresh.
- **Tolerate a bad line.** `load` skips lines that do not parse — the last line
  is the one a crash truncates, and losing the chart over it would be a poor
  trade. Only a rewrite (the trim) goes through `atomic_file`.
- **Stay bounded, and check the bound cheaply.** Trimming is triggered by a
  `stat` against a size threshold, not by counting lines on every append.
- **Best-effort at the call site.** A history that cannot be written must not
  turn a successful reading into a failed command; the same rule the config
  persistence paths in `tui::app` already follow.

## Presentation

- **One colour scale, one owner.** Every status colour comes from
  `Theme::status_color`. A surface that needs green/yellow/orange/red asks for
  it; it never picks its own, or the card border and the gauge beside it drift
  apart.
- **Glyphs never carry meaning alone.** Every icon sits next to its own label.
  Nerd Font codepoints live in the Private Use Area and draw as empty boxes in
  an unpatched terminal, so each one is paired with a plain-Unicode stand-in
  (`sensors::Icons`) and the theme picks between them — `mono` is the TTY-safe
  theme, so it is also the plain-icon theme. A render test asserts `mono` emits
  no PUA codepoints at all; it caught the powerline separator, which had been
  slipping through since before the icons existed.
- **Animate only what is happening.** `TuiApp::is_animating` gates the
  animation clock, and the runtime advances it only when that holds — on the
  spare moment when an input poll times out. An idle dashboard is never
  repainted. Anything animated reads `animation_frame` rather than a wall
  clock, which is also what keeps rendering a pure function of state and the
  render tests deterministic.
- **Widgets from crates, decisions from us.** `tui-slider`, `tui-spinner`, and
  `ratatui-toaster` draw; what to say, which band a value is in, and when an
  alert stops showing are ours. `ui::toast` builds its engine per frame from
  app state rather than holding one between frames, so no widget owns state the
  model should.

## Testing

- Every behavior change ships with a test; every bug fix ships with the test
  that would have caught it.
- Tests run headless with no setup: `cargo test`. No network, no seeded state,
  no manual terminal.
- Write against seams, not internals. The event loop is tested through
  `TerminalRuntime` and `MeasureFetchWorker` fakes with an injected clock, so
  refresh-timing tests are deterministic instead of sleeping.
- Group a large suite by what it exercises (`schedule`, `loop_flow`,
  `failure_paths`) rather than by one file per source file.
- A test that pins prose (like `tests/release_contract_docs.rs`) must match on
  meaning, not formatting — normalize whitespace so rewrapping a paragraph
  does not fail the build, and pin the facts rather than one sentence's
  phrasing.

## Before Committing

```sh
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Formatting is `cargo fmt`'s decision, not a review topic.
