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
