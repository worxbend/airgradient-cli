# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright==1.63.0"]
# ///
"""Exercise the staged GitHub Pages site in Chromium, including its subpath.

Run: uv run scripts/test-site.py
Install browser first: uv run --with playwright==1.63.0 playwright install chromium
Optional accessibility audit: --axe /path/to/axe.min.js
Screenshots are written to target/site-preview/, never published automatically.
"""
from __future__ import annotations

import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import tempfile
from threading import Thread
import tomllib

from playwright.sync_api import Browser, Page, expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = ROOT / "target/site-preview"


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        pass


def assert_layout(page: Page) -> None:
    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth"), "Horizontal overflow"
    assert page.locator("h1").count() == 1
    expect(page.locator("#install")).to_contain_text("Restart your shell before the next step")
    expect(page.locator("#config")).to_contain_text("Existing history remains readable")
    for image in page.locator("img:visible").all():
        image.scroll_into_view_if_needed()
        expect(image).to_have_js_property("complete", True)
        assert image.evaluate("""image => image.naturalWidth > 0
            && image.naturalWidth === Number(image.getAttribute('width'))
            && image.naturalHeight === Number(image.getAttribute('height'))"""), "Missing image or stale dimensions"
    assert page.evaluate("""() => [...document.querySelectorAll('a[href^="#"]')]
        .every(a => document.getElementById(a.hash.slice(1)))"""), "Broken anchor"


def audit(page: Page, axe: Path | None) -> None:
    if axe is None:
        return
    # CDP evaluation avoids weakening the production CSP to load test tooling.
    page.evaluate(axe.read_text())
    result = page.evaluate("""async () => await axe.run(document, {
      runOnly: {type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21aa']}
    })""")
    assert not result["violations"], [
        (v["id"], [node["target"] for node in v["nodes"]]) for v in result["violations"]
    ]


def check_responsive(browser: Browser, base: str, axe: Path | None) -> None:
    for width in [320, 390, 768, 1024, 1440, 1920]:
        context = browser.new_context(viewport={"width": width, "height": 1000}, reduced_motion="reduce")
        page = context.new_page()
        errors: list[str] = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.on("console", lambda msg: errors.append(msg.text) if msg.type in ("error", "warning") else None)
        page.on("response", lambda response: errors.append(f"HTTP {response.status}: {response.url}") if response.status >= 400 else None)
        external: list[str] = []
        page.on("request", lambda req: external.append(req.url) if not req.url.startswith(base) else None)
        page.goto(base)
        page.evaluate("document.fonts.ready")
        assert_layout(page)
        page.evaluate("window.scrollTo(0, 0)")
        expect(page.locator("#airflow")).to_have_attribute("data-motion", "still")
        audit(page, axe)
        page.screenshot(path=str(ARTIFACTS / f"site-{width}.png"), full_page=True)
        assert not errors, errors
        assert not external, external
        context.close()
        print(f"PASS responsive {width}px; images, anchors, CSP and console" + ("; axe" if axe else ""))


def check_gallery(browser: Browser, base: str, axe: Path | None) -> None:
    context = browser.new_context(reduced_motion="reduce")
    page = context.new_page()
    page.goto(base)
    page.locator("#tab-dashboard").focus()
    for key, name in [("ArrowRight", "history"), ("End", "themes"), ("Home", "dashboard"), ("ArrowLeft", "themes")]:
        page.keyboard.press(key)
        expect(page.locator(f"#tab-{name}")).to_be_focused()
        expect(page.locator(f"#tab-{name}")).to_have_attribute("aria-selected", "true")
        expect(page.locator(f"#panel-{name}")).to_be_visible()
        assert page.locator('[role="tabpanel"]:visible').count() == 1
        assert_layout(page)
        audit(page, axe)
    page.locator("#tab-dashboard").click()
    for theme, file in [("nord", "tui-nord.png"), ("catppuccin", "tui-catppuccin.png"), ("default", "tui-dashboard.png")]:
        button = page.locator(f'[data-theme="{theme}"]')
        button.click()
        expect(button).to_have_attribute("aria-pressed", "true")
        expect(page.locator("#dashboard-image")).to_have_attribute("src", base + "assets/screenshots/" + file)
        assert_layout(page)
    context.close()
    print("PASS gallery keyboard navigation and all three dashboard themes")


def check_copy(browser: Browser, base: str) -> None:
    context = browser.new_context(permissions=["clipboard-read", "clipboard-write"])
    page = context.new_page()
    page.goto(base)
    for button in page.locator("[data-copy]").all():
        target_id = button.get_attribute("data-copy")
        expected = page.locator(f"#{target_id}").inner_text().strip()
        button.click()
        expect(page.locator("#toast")).to_contain_text("Command copied")
        assert page.evaluate("navigator.clipboard.readText()") == expected
    context.close()
    context = browser.new_context()
    context.add_init_script("""Object.defineProperty(navigator, 'clipboard', {
      value: {writeText: async () => { throw new Error('Test: denied'); }}
    });""")
    page = context.new_page()
    page.goto(base)
    page.locator('[data-copy="launch-command"]').click()
    expect(page.locator("#toast")).to_contain_text("Copy unavailable")
    assert page.evaluate("getSelection().toString()") == "airgradient-cli --tui"
    context.close()
    print("PASS real clipboard contents and denied-copy selection fallback")


def check_fallbacks(browser: Browser, base: str, axe: Path | None) -> None:
    context = browser.new_context(java_script_enabled=False, viewport={"width": 320, "height": 900})
    page = context.new_page()
    page.goto(base)
    assert page.locator('[role="tabpanel"]:visible').count() == 3
    assert page.locator("[data-copy]:visible").count() == 0
    assert_layout(page)
    page.locator("summary").first.click()
    expect(page.locator("details").first).to_have_attribute("open", "")
    # axe uses timers; keep this context genuinely script-disabled.
    page.screenshot(path=str(ARTIFACTS / "site-no-js.png"), full_page=True)
    context.close()
    context = browser.new_context(reduced_motion="reduce")
    context.add_init_script("""Object.defineProperty(window, 'localStorage', {
      get() { throw new Error('Test: storage denied'); }
    });""")
    page = context.new_page()
    page.goto(base)
    expect(page.locator("#view-tabs")).to_be_visible()
    page.locator("#tab-history").click()
    expect(page.locator("#panel-history")).to_be_visible()
    audit(page, axe)
    context.close()
    print("PASS no-JS content, FAQ and denied-storage enhancement")


def check_motion(browser: Browser, base: str) -> None:
    context = browser.new_context(viewport={"width": 1440, "height": 1000}, reduced_motion="no-preference")
    page = context.new_page()
    page.goto(base)
    canvas = page.locator("#airflow")
    expect(canvas).to_have_attribute("data-motion", "running")
    page.locator("#motion-toggle").click()
    expect(canvas).to_have_attribute("data-motion", "still")
    first = canvas.screenshot()
    page.wait_for_timeout(150)
    assert first == canvas.screenshot(), "Paused art still changing"
    page.locator("#motion-toggle").click()
    expect(canvas).to_have_attribute("data-motion", "running")
    page.locator("#install").scroll_into_view_if_needed()
    expect(canvas).to_have_attribute("data-motion", "still")
    page.locator("#top").scroll_into_view_if_needed()
    expect(canvas).to_have_attribute("data-motion", "running")
    page.emulate_media(reduced_motion="reduce")
    expect(canvas).to_have_attribute("data-motion", "still")
    context.close()
    print("PASS pause/play, offscreen suspension and live reduced-motion change")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--axe", type=Path)
    args = parser.parse_args()
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    assert f"v{version} is here" in (ROOT / "site/index.html").read_text()
    with tempfile.TemporaryDirectory(prefix="airgradient-site-") as temporary:
        stage = Path(temporary) / "airgradient-cli"
        shutil.copytree(ROOT / "site", stage)
        shutil.copy2(ROOT / "scripts/install.sh", stage / "install.sh")
        server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=temporary))
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        base = f"http://127.0.0.1:{server.server_port}/airgradient-cli/"
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch()
                check_responsive(browser, base, args.axe)
                check_gallery(browser, base, args.axe)
                check_copy(browser, base)
                check_fallbacks(browser, base, args.axe)
                check_motion(browser, base)
                browser.close()
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
    print(f"Site checks passed. Captures: {ARTIFACTS}")


if __name__ == "__main__":
    main()
