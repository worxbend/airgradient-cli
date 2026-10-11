# TUI design system

The dashboard is a projection of `TuiApp`: styling and layout do not change
sensor thresholds, command grammar, persistence, navigation, or acquisition.
The wide layout prioritizes the AQI overview, then indoor comfort, gases, and
particulates. Compact layouts preserve readings and exit controls rather than
shrinking the entire dashboard into unreadable cards.

## Tokens

`src/tui/theme/palette.rs` contains palette data; `theme/tokens.rs` derives
surfaces and interaction styles. Components must use semantic tokens rather
than literal widget colors.

| Role | API | Purpose |
| --- | --- | --- |
| Page | `bg`, `page_style()` | Quiet background between panels |
| Panel | `surface()`, `surface_style()`, `panel_style()` | Main content cards |
| Raised | `surface_hi()`, `raised_style()` | Dialogs and notifications |
| Selected | `selected_style()` | Visible, bold selected row |
| Track | `track()` | Unfilled meter foreground |
| Status card | `card_surface(status)` | Restrained status-tinted panel fill |
| Status | `status_color(status)` | Shared sensor severity mapping |
| Key | `key_style()` | Keyboard affordances |
| Badge | `badge_style(color)` | Compact textual state on a raised fill |
| Text | `fg`, `muted`, `label_style()`, `value_style()` | Content hierarchy |
| Focus | `border_focus`, `active_border_style()` | Active surface boundary |

`VisualState` describes **Normal, Focused, Selected, Active, Disabled, Success,
Warning, Error, Info** independently of sensor severity. Use `state_style()`
for interaction intent and `status_color()` for measurement classification;
neither changes the domain thresholds. Severity must have a textual label as
well as a color.

`Theme::is_background()` is the authoritative fill whitelist. All rendered
cells, including spaces and chart canvases, must satisfy it. A dialog clearing
an area must repaint the entire area with its raised fill. Chart canvases must
match the surrounding panel, not the page. The `mono` palette leaves reset
backgrounds to the terminal and avoids Nerd Font-dependent content; never
resolve `Color::Reset` to the active theme's background to conceal a fill bug.

## Components and layout

- `ui/components.rs`: panels, sections, tabs, badges, key hints, dialogs, and
  log-style feedback. Reuse these primitives so focus and spacing agree.
- `ui/status_line.rs`: app identity, acquisition state, refresh timing, and
  navigation hints. These are display affordances, not a new routing layer.
- `ui/air_monitor*.rs`, `dashboard.rs`, `gauge.rs`: wide and compact measurement
  surfaces, value hierarchy, trend context, and bounded meters.
- `ui/settings.rs`: theme preview and configuration fields. Selected rows must
  scroll into view; click targets use absolute item indices, not visible-row
  indices. Esc retains the existing cancel semantics.
- `ui/chart/`: metric selection, recorded range, and status-colored history.
  Missing readings remain missing rather than invented zeroes.
- `ui/command_palette.rs`: floating command reference and editable input. The
  tail and cursor must remain visible when Unicode text exceeds its width.
- `ui/leader.rs`, `toast.rs`, `splash.rs`: keyboard help, severity notifications,
  and startup presentation. Animation is frame-driven; alert expiry belongs
  to the app/runtime, not a renderer's private timer.
- `ui/chrome.rs`: exit controls, connection errors, and post-command feedback.
  A submitted command's result must remain visible after the palette closes.

Use Ratatui terminal-cell measurements rather than UTF-8 length. Wide glyphs,
combining marks, zero-sized resize frames, and short terminal heights are part
of the rendering contract. The dashboard's documented minimum remains 36×20;
settings/history still render defensively at smaller sizes. One-cell gutters
and outer margins are only used when there is space to afford them.

`ui::draw_with_hits()` records targets during rendering. Tests locate visible
labels in the resulting buffer and check those exact cell coordinates against
`HitMap`; independent guessed rectangles can pass while real clicks are wrong.

## Headless preview gallery

```sh
cargo run --locked --example tui_preview
# Other palette, optional output directory:
cargo run --locked --example tui_preview -- target/tui-preview-mono mono
# All palettes (larger HTML file):
cargo run --locked --example tui_preview -- target/tui-preview-all all
```

Open `target/tui-preview/index.html` in a browser. The same directory contains
one UTF-8 `.txt` cell-buffer dump per view and palette. Outputs live under the
ignored `target/` directory by default; no screenshot binaries are committed.
The example adds no dependencies and does not need a TTY, device, network,
credentials, or configuration file.

The gallery covers wide and compact dashboards, themes, configuration,
history, command palette, keyboard help, worsening and recovery alerts,
connection error, submitted-command feedback, and splash. Each frame is drawn
by the **production renderer** into a Ratatui `TestBackend`. HTML is serialized
from its actual cell buffer, not recreated with independent card markup.

Every gallery heading and text export explicitly identifies **synthetic
fixtures — not live sensor data**. The device name is `synthetic.invalid`.
Fixtures use fixed acquisition timestamps, a fixed animation frame, and a
repeatable series; both persistence paths remain `None`. Alert fixtures use
real app transitions, and feedback fixtures use real command submission.

### Capture fidelity and time

HTML keeps each cell's original foreground, background, and modifier metadata
in `data-*` attributes. Separate fill and glyph layers preserve backgrounds
without painting over wide characters. RGB values are exact; indexed colors
use the declared xterm palette. Reset colors use the example terminal's
`--terminal-fg` and `--terminal-bg`, **not** the selected theme. Bold, italic,
dim, underline, strikeout, reverse, and hidden styles are represented; blinking
modifiers are captured in their visible phase rather than adding browser-time
animation. A locally installed compatible Nerd Font may be required for
private-use icons. Browser font rasterization is not a terminal pixel snapshot.

The example calls `ui::draw_with_hits_at()` with the fixed acquisition time as
its explicit render clock. Both the relative-age formatter and history plot
therefore use that captured reference rather than `SystemTime::now()`; normal
runtime wrappers still capture the current time. The bounded history retains
48 readings spanning 23 minutes 30 seconds before the reference. The regression
test checks that axis and compares captures across a clock tick;
if it fails, fix the renderer's clock input, not the stored measurements or
exported cells. Merely drawing twice immediately can hide wall-clock drift
because long elapsed durations are rounded to minutes.

## Verification

```sh
cargo test --locked --test tui_design
cargo test --locked --test tui_settings_history --test tui_render
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo fmt --check
```

`tui_design` shares the preview fixtures rather than maintaining a second set
of fabricated data. It sweeps every palette across representative sizes and
views/overlays for unapproved backgrounds; checks zero/tiny sizes; locates the
last selected theme's real label and click target; exercises long wide and
combining Unicode palette input; checks visible success/error feedback after
submission without config writes; and validates HTML escaping/style fidelity
and preview-clock stability.

When the local rustup shim cannot sync the pinned toolchain, the installed
compiler can be invoked explicitly with `rustup run 1.96.0 cargo ...` and
`rustup run 1.96.0 rustfmt --edition 2024 examples/tui_preview.rs tests/tui_design.rs`.
Do not update the project toolchain to work around a local component conflict.
Headless buffer coverage does not replace the repository's separate real-PTY
input/cleanup tests or a live-device acquisition test.
