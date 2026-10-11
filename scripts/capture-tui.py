#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright==1.63.0", "pillow==12.3.0", "fonttools==4.66.1"]
# ///
"""Capture real TestBackend HTML cells, never reconstruct the dashboard.

See docs/screenshot-capture.md for setup, provenance, and fidelity limits.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import importlib.metadata
import json
import math
from pathlib import Path
import subprocess

from fontTools.ttLib import TTFont
from PIL import Image
from playwright.sync_api import Browser, Page, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
SHOTS = (
    ("default", "wide", "tui-dashboard.png"),
    ("default", "history", "tui-history.png"),
    ("default", "themes", "tui-themes.png"),
    ("nord", "wide", "tui-nord.png"),
    ("catppuccin-mocha", "wide", "tui-catppuccin.png"),
)


def command(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def font_face(family: str, bold: bool) -> tuple[str, dict, set[int]]:
    """Resolve explicit local faces and refuse silent fontconfig fallback."""
    pattern = f"{family}:style={'Bold' if bold else 'Regular'}"
    names, filename = command("fc-match", pattern, "-f", "%{family}\n%{file}").splitlines()
    if family not in names.split(","):
        raise RuntimeError(f"Install {family!r}; fontconfig instead selected {names!r}")
    path = Path(filename)
    with TTFont(path) as font:
        coverage = set(font.getBestCmap())
        version = font["name"].getDebugName(5)
    encoded = base64.b64encode(path.read_bytes()).decode("ascii")
    weight = 700 if bold else 400
    css = (
        "@font-face{font-family:CaptureMono;"
        f"src:url(data:font/ttf;base64,{encoded});font-weight:{weight};"
        "font-style:normal;font-display:block;}"
    )
    return css, {"file": path.name, "sha256": digest(path), "version": version}, coverage


def render_galleries(toolchain: str | None, directory: Path) -> None:
    cargo = ["rustup", "run", toolchain, "cargo"] if toolchain else ["cargo"]
    for theme in dict.fromkeys(shot[0] for shot in SHOTS):
        subprocess.run(
            [*cargo, "run", "--locked", "--example", "tui_preview", "--",
             str(directory / theme), theme],
            cwd=ROOT, check=True,
        )


def open_gallery(browser: Browser, path: Path, css: str) -> Page:
    page = browser.new_page(
        viewport={"width": 1600, "height": 1100}, device_scale_factor=1,
        color_scheme="dark", locale="en-US", timezone_id="UTC",
        reduced_motion="reduce",
    )
    page.goto(path.as_uri(), wait_until="load")
    page.add_style_tag(content=css + ".terminal{font-family:CaptureMono;font-size:16px}")
    page.evaluate("async () => { await document.fonts.load('16px CaptureMono');"
                  "await document.fonts.load('700 16px CaptureMono');"
                  "await document.fonts.ready; }")
    return page


def select_scene(page: Page, scene: str):
    sections = page.locator("section").filter(
        has=page.locator("h2", has_text=f" / {scene} / 144×46")
    )
    if sections.count() != 1:
        raise RuntimeError(f"Expected exactly one wide {scene!r} scene")
    terminal = sections.locator(".terminal")
    for layer in (".cell", ".glyph"):
        if terminal.locator(layer).count() != 144 * 46:
            raise RuntimeError(f"Incomplete TestBackend {layer} layer")
    return terminal


def verify_glyphs(terminal, coverage: list[set[int]]) -> int:
    runs = terminal.locator(".glyph").evaluate_all(
        "nodes => nodes.map(n => [n.textContent, n.style.fontWeight === 'bold'])"
    )
    missing = {ord(char) for text, bold in runs for char in text
               if ord(char) not in coverage[int(bold)]}
    if missing:
        raise RuntimeError(f"Missing font glyphs: {[f'U+{cp:04X}' for cp in sorted(missing)]}")
    return len({char for text, _ in runs for char in text})


def capture(page: Page, scene: str, output: Path, coverage: list[set[int]]) -> dict:
    terminal = select_scene(page, scene)
    glyphs = verify_glyphs(terminal, coverage)
    bounds = terminal.bounding_box()
    if bounds is None:
        raise RuntimeError("Terminal has no browser layout bounds")
    expected = (math.ceil(bounds["width"]), math.ceil(bounds["height"]))
    # Integer crop origin avoids scene-dependent rounding at fractional scroll offsets.
    original_style = terminal.get_attribute("style")
    terminal.evaluate("node => Object.assign(node.style, "
                      "{position:'fixed',left:'0',top:'0',zIndex:'999'})")
    try:
        page.screenshot(path=str(output), animations="disabled", scale="css",
                        clip={"x": 0, "y": 0, "width": expected[0], "height": expected[1]})
    finally:
        terminal.evaluate("(node, style) => node.setAttribute('style', style)", original_style)
    with Image.open(output) as image:
        image.load()
        if image.format != "PNG" or image.size != expected:
            raise RuntimeError(f"Unexpected PNG dimensions/format: {output}")
        colors = image.convert("RGB").getcolors(image.width * image.height)
        if colors is None or len(colors) < 16:
            raise RuntimeError(f"Empty or suspiciously flat screenshot: {output}")
        # Lossless recompression only; never repaint, quantize, or edit pixels.
        image.save(output, optimize=True)
    return {"file": output.name, "width": expected[0], "height": expected[1],
            "bytes": output.stat().st_size, "sha256": digest(output),
            "unique_glyphs_checked": glyphs}


def source_hashes() -> dict[str, str]:
    paths = [ROOT / "examples/tui_preview.rs", ROOT / "Cargo.lock"]
    paths.extend(sorted((ROOT / "src").rglob("*.rs")))
    return {str(path.relative_to(ROOT)): digest(path) for path in paths}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain", help="Explicit rustup toolchain, e.g. 1.96.0")
    parser.add_argument("--font-family", default="JetBrainsMono Nerd Font Mono")
    parser.add_argument("--output", type=Path, default=ROOT / "site/assets/screenshots")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    fonts = [font_face(args.font_family, bold) for bold in (False, True)]
    gallery = ROOT / "target/tui-capture"
    render_galleries(args.toolchain, gallery)
    screenshots = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(args=["--force-color-profile=srgb"])
        version = browser.version
        for theme in dict.fromkeys(shot[0] for shot in SHOTS):
            page = open_gallery(browser, gallery / theme / "index.html",
                                "".join(font[0] for font in fonts))
            try:
                for shot_theme, scene, filename in SHOTS:
                    if theme == shot_theme:
                        record = capture(page, scene, output / filename,
                                         [font[2] for font in fonts])
                        screenshots.append({"theme": theme, "scene": scene, **record})
                        print(f"{filename}: {record['width']}×{record['height']}, {record['bytes']} bytes")
            finally:
                page.close()
        browser.close()
    if len(screenshots) != len(SHOTS) or len({(s["width"], s["height"]) for s in screenshots}) != 1:
        raise RuntimeError("Incomplete capture set or inconsistent wide dimensions")
    manifest = {
        "synthetic_fixture": True, "live_hardware": False,
        "renderer": "examples/tui_preview.rs → Ratatui TestBackend → HTML → Chromium PNG",
        "terminal_cells": [144, 46], "font_size_px": 16, "device_scale_factor": 1,
        "fixed_unix_time": 1700000000, "animation_frame": 8,
        "chromium": version, "font_family": args.font_family,
        "fonts": [font[1] for font in fonts],
        "packages": {name: importlib.metadata.version(name)
                     for name in ("playwright", "pillow", "fonttools")},
        "source_sha256": source_hashes(), "screenshots": screenshots,
    }
    (output / "capture-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
