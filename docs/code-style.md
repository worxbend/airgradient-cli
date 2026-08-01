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
- **Known deviation:** `src/tui/runtime/tests/mod.rs` is ~520 lines of shared
  test fakes. It is a single cohesive harness (the doubles for the two runtime
  seams) and splitting it would require widening the visibility of a dozen
  fields purely to satisfy the line count. If it grows further, split the
  fetcher doubles out and mark them `pub(super)`.

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

## Errors

- Errors carry context: the path, the value, the expected range. Compare
  `ConfigError::RefreshIntervalOutOfRange`, which names the bounds *and* what
  it got.
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
  before it became `start_refresh_if_configured`.

## Shared Values

Presentation constants belong to the layer that owns the concept, not to each
renderer. `MISSING_VALUE` and `Trend::display_symbol` live in
`src/sensors/presentation.rs` because the text, JSON, and TUI outputs must all
show the same placeholder for the same absent reading.

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
