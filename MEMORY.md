[pattern] A shared metric presentation spine keeps text, JSON, and future TUI output aligned on labels, units, statuses, formatted values, and trends.
[anti-pattern] Accepting a CLI flag before the backing feature exists creates a user-visible contract; `--tui` should either work or be clearly documented as pending.
[anti-pattern] Local-device HTTP clients still need explicit timeouts; LAN requests can hang indefinitely when devices are offline or half-open.
[learning] Air-quality parsers should validate numeric domains, not only numeric syntax, because impossible negative PM values can otherwise render as false good air quality.
[learning] Desktop-compatible config writes need an explicit unknown-field policy early, since simple typed serialization silently drops future sibling-app fields.
[learning] Terminal color policy has two surfaces: rendered stdout and diagnostic stderr need separate non-TTY handling.
[anti-pattern] Documenting destructive config rewrite semantics is useful, but preserving unknown sibling fields is the safer compatibility default for shared app config files.
