# TUI screenshot capture

The PNGs in `site/assets/screenshots/` show **synthetic fixtures, not live
sensor data or real-hardware acquisition**. They are genuine browser captures
of the current production TUI renderer's Ratatui `TestBackend` cell buffers.
They are not hand-drawn terminal illustrations or screenshots of a physical
terminal emulator.

## Assets

All five images use the same **144×46 terminal cells / 1440×957 pixels**:

| File | Theme ID | Preview scene |
| --- | --- | --- |
| `tui-dashboard.png` | `default` (AirGradient) | `wide` |
| `tui-history.png` | `default` | `history` |
| `tui-themes.png` | `default` | `themes` |
| `tui-nord.png` | `nord` | `wide` |
| `tui-catppuccin.png` | `catppuccin-mocha` | `wide` |

Use a nearby caption such as “Production TUI renderer with deterministic
synthetic readings” when publishing these images. The dashboard's
`synthetic.invalid` URL is deliberate, not a real device address.

## Reproduce

Requirements: the repository's Rust toolchain, Python 3.11+, Linux fontconfig
(`fc-match`), and **JetBrainsMono Nerd Font Mono** regular and bold faces.
Install the font from [Nerd Fonts](https://www.nerdfonts.com/font-downloads)
if needed. Font files are not redistributed in the website.

Run from the repository root. The script declares pinned Python dependencies
in its PEP 723 header; a plain virtual environment works without `uv`:

```sh
# Use your scratch directory, not a system-wide Python installation.
: "${TMPDIR:?Set TMPDIR to your scratch directory first}"
python3 -m venv "$TMPDIR/tui-capture-venv"
"$TMPDIR/tui-capture-venv/bin/python" -m pip install \
  playwright==1.63.0 pillow==12.3.0 fonttools==4.66.1
"$TMPDIR/tui-capture-venv/bin/python" -m playwright install chromium
"$TMPDIR/tui-capture-venv/bin/python" scripts/capture-tui.py
```

Alternatively, after installing Chromium with the same Playwright version,
run `uv run scripts/capture-tui.py`. If the local rustup shim cannot sync the
pinned compiler, use the repository's installed-toolchain workaround:

```sh
"$TMPDIR/tui-capture-venv/bin/python" scripts/capture-tui.py --toolchain 1.96.0
```

The script invokes `cargo run --locked --example tui_preview` for each required
theme, leaves inspectable HTML/text exports under ignored
`target/tui-capture/<theme>/`, and writes the five PNGs plus
`capture-manifest.json`. `--output PATH` redirects the screenshots and manifest
for comparison without replacing the published assets. It neither edits Rust
source nor reads/writes user configuration or history.

## Fidelity and provenance

- `examples/tui_preview.rs` builds the fixtures and calls
  `ui::draw_with_hits_at()` into the real Ratatui `TestBackend`. Capture does
  not reconstruct widgets, rewrite strings, or change the telemetry.
- Acquisition and rendering share fixed Unix time `1700000000`; dashboard
  animation frame is `8`. History contains the fixture's 48 retained readings.
  Relative ages and chart axes therefore do not depend on capture wall-clock time.
- The example exports actual cell positions, RGB/ANSI/reset colors and text
  modifiers in separate background and glyph layers. Capture preserves these
  values, including its declared terminal reset palette. Blink is visible and
  static, as specified by the exporter.
- Chromium uses an sRGB color profile, device scale factor 1 and 16px
  JetBrainsMono Nerd Font Mono with the exporter's 1.3 line height. The script
  embeds the exact local regular/bold font files into the browser page, waits
  for them to load, and rejects any glyph absent from the relevant font cmap.
  Font family fallback is not silently accepted.
- Only the font and screenshot placement are overridden. The existing terminal
  element is placed at an integer viewport origin before cropping, avoiding
  scene-dependent fractional scrolling/clip rounding. There is no extra fake
  window chrome. PNG optimization is lossless, without palette quantization,
  recoloring, or repainting.
- Browser text rasterization is not terminal-emulator pixel equivalence.
  Pixel-identical regeneration depends on the same fonts, browser and platform.
  The manifest records browser/package versions, font names/versions/hashes,
  source hashes, image dimensions, byte counts and PNG SHA-256 hashes.

The checked-in set was rendered using Chromium `153.0.8010.12`, Playwright
`1.63.0`, and JetBrains Mono `2.304` patched with Nerd Fonts `3.5.1` (regular
and bold). The five PNGs total 482550 bytes. A second independent capture
produced identical image hashes and an identical manifest. All five images
were visually inspected; glyph coverage, complete cell layers, nonempty
pixel content and equal dimensions are also verified by the capture script.
`cargo test --locked --test tui_design` passed all seven tests, including
fixed-clock and exported-style regressions.

The optional mono image is not published: its non-Nerd Unicode symbol set
includes glyphs absent from this particular font. Capturing it would require
an explicitly verified fallback font rather than accepting missing-glyph boxes.
For the broader gallery contract see [TUI design](tui-design.md); its default
ignored HTML/text exports are separate from these intentionally published PNGs.
