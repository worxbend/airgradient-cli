# Microsite: room to breathe

## Design contract

The page helps an AirGradient owner understand the terminal dashboard, see the
real interface, and install it. Copy stays short; the README and usage guide
carry the reference material. Release facts come from Cargo.toml, the published
GitHub release, and docs/releases/v0.4.0.md.

Palette: mist `#f4f9f7`, paper `#ffffff`, ink `#123d38`, fern `#25745f`,
sea glass `#d9ede5`, slate `#506b65`. Use dark terminal captures as the product,
not a dark SaaS page around it.

Typography: self-hosted Roboto Flex, oversized but quiet, with tight display
spacing; self-hosted Roboto Mono only for commands and key bindings. No external
fonts, analytics, component runtime, or network-fetched release metadata.

Layout: a spacious asymmetric introduction beside a custom airflow sculpture;
a wide, readable terminal gallery; a compact feature strip; an editorial
history/local-first section; a sequential installation area and native FAQs.
Align text left. Keep prose below 65 characters per line. Mobile stacks in
reading order without hiding navigation or essential controls.

The memorable element is a current of air drawn into a translucent sphere.
Canvas particles follow the SVG's shape, never pretend to be measured air data.
Reject stock feature-card grids, dashboard-shaped marketing mockups, meaningless
counters, animated section entrances, and distracting cursor-following effects.

## Motion and accessibility

Only the airflow sculpture moves automatically. A visible pause control stops
it. Reduced-motion users get a still composition; the loop stops offscreen and
in hidden tabs. Cap device pixel ratio and frame rate. No scroll hijacking.

Keep install commands, screenshots, navigation, and FAQs usable without JS.
Enhance the gallery with keyboard-operated tabs and theme preview buttons.
Use visible focus rings, named navigation, image descriptions, explicit demo
captions, and an announced copy result. If copying fails, select the command and
explain how to copy manually. Never call clipboard success before it resolves.

## Verification and deployment

The site remains static: `.github/workflows/pages.yml` stages `site/` and copies
`scripts/install.sh`. All paths must work under `/airgradient-cli/`, not only at
the domain root. No application runtime or release changes are required.

Check desktop and 320px mobile, keyboard gallery navigation, all preview images,
copy success/failure, disabled JS, reduced motion, missing storage access,
console/CSP errors, accessibility, and local asset/anchor integrity. Captures
must originate from the current Ratatui renderer and be labeled synthetic.
Publishing requires an explicit commit/push request; local verification does not
establish that the public Pages deployment changed.

### Run the browser checks

```sh
uv run --with playwright==1.63.0 playwright install chromium
uv run scripts/test-site.py
```

The script stages the same site and installer as Pages, serves them under
`/airgradient-cli/`, and exercises Chromium at 320, 390, 768, 1024, 1440 and
1920 pixels. It checks image loading, console/CSP errors, keyboard tabs, theme
previews, real clipboard output, denied-copy fallback, disabled JavaScript,
blocked storage, offscreen animation suspension and reduced motion. It saves
page captures to `target/site-preview/`.

Pass `--axe /path/to/axe.min.js` to add WCAG A/AA checks. The Site checks workflow
installs pinned Playwright and axe-core versions and runs the same script on
site-related pull requests and pushes. These are test-only dependencies; the
published page stays dependency-free. Browser automation does not replace
human screen-reader testing or establish a medical interpretation of readings.
