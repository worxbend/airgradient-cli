[pattern] A shared metric presentation spine keeps text, JSON, and future TUI output aligned on labels, units, statuses, formatted values, and trends.
[anti-pattern] Accepting a CLI flag before the backing feature exists creates a user-visible contract; `--tui` should either work or be clearly documented as pending.
[anti-pattern] Local-device HTTP clients still need explicit timeouts; LAN requests can hang indefinitely when devices are offline or half-open.
[learning] Air-quality parsers should validate numeric domains, not only numeric syntax, because impossible negative PM values can otherwise render as false good air quality.
[pattern] Desktop-compatible config mutation should overlay typed known fields onto the existing raw JSON object to preserve future sibling-app fields.
[learning] Diagnostic output is easier to keep concise and non-colored when `main` owns error rendering instead of delegating to a rich report handler.
[learning] Tolerant config display needs raw-field handling for every known field, not only URL normalization; typed deserialization can still block partial repair.
[learning] Parser validation should be integrated into candidate search; validating only after selecting the first matching field can hide valid fallback fields.
[pattern] TUI work is easier to verify when state transitions are pure and tested before terminal drawing or event-loop code exists.
[anti-pattern] Integration tests should import the crate API, not source files by path, or they duplicate unit tests and miss public-surface regressions.
[pattern] When compatibility repair has an intentional hard boundary, document the boundary and its reason so future work does not treat it as an accidental parser failure.
[anti-pattern] A TUI event loop that awaits network fetches inline makes keyboard responsiveness depend on HTTP timeout rather than terminal event handling.
[learning] Terminal cleanup confidence requires a testable terminal adapter or harness; asserting a cleanup plan does not prove cleanup happens after runtime errors.
[pattern] A small runtime adapter plus harness can verify TUI draw, event, fetch, and cleanup behavior without requiring a real terminal in every test.
[learning] Non-blocking TUI fetches need lifecycle ownership too; responsiveness is incomplete if pending tasks cannot be cancelled or joined on exit.
[anti-pattern] Advancing scheduler time by intended poll durations instead of observed wall-clock time makes event-loop tests pass while leaving drift and early-fire edge cases under-specified.
[pattern] TUI render state should expose fetch lifecycle explicitly so the UI can distinguish first fetch, refresh with stale data, success, failure, and missing config.
[pattern] Blocking terminal APIs can coexist with async fetch work when poll/read are isolated behind `spawn_blocking` and covered by current-thread runtime tests.
[learning] Aborting a Tokio `JoinHandle` requests cancellation; without awaiting or observing the handle, cleanup owns cancellation intent rather than proof of task termination.
[pattern] PTY integration tests are the right proof for real TUI startup and keyboard exit, but they should be reported as conditional coverage when platform support is skippable.
[learning] Awaited cancellation can still lose diagnostic value if cancellation or task-panic errors are dropped behind a primary runtime error; preserve secondary failure context deliberately.
[pattern] Compact TUI support is clearer when a minimum terminal size and below-minimum fallback are explicit contracts rather than incidental layout behavior.
[learning] Conditional test coverage reported only through passing-test stderr can be invisible in normal CI output; surface skip state through summaries or explicit reporting.
[anti-pattern] Conditional integration-test helpers must distinguish missing platform capability from test infrastructure failure; broad skip results can hide broken CI or missing binaries.
[learning] PTY closed-read error codes are platform semantics, not universal constants; scope raw OS error handling to the OS where the code meaning is known.
[pattern] Conditional integration-test result types should carry only the skippable capability gap, while infrastructure failures stay outside the skip variant and fail loudly.
