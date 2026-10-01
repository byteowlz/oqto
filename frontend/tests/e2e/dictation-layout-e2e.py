"""Measure real compositor/textarea stability without microphone or credentials.
Run against a local Vite server: python3 tests/e2e/dictation-layout-e2e.py
"""
import argparse
import json
from pathlib import Path
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument("--url", default="http://127.0.0.1:3018/dictation-layout.html")
parser.add_argument("--output", default="/tmp/oqto-dictation-layout")
args = parser.parse_args()
output = Path(args.output)
output.mkdir(parents=True, exist_ok=True)
results = []
with sync_playwright() as playwright:
    browser = playwright.chromium.launch(headless=True)
    for width in (1280, 390):
        for theme in ("light", "dark"):
            page = browser.new_page(viewport={"width": width, "height": 900})
            errors = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.goto(args.url)
            page.get_by_role("textbox", name="Draft").wait_for()
            page.evaluate("document.fonts.ready")
            page.evaluate("theme => document.documentElement.classList.toggle('dark', theme === 'dark')", theme)
            selectors = ["[data-fixture-chat]", "[data-fixture-composer]", "textarea"]
            initial = [page.locator(selector).bounding_box() for selector in selectors]
            editor = page.get_by_role("textbox", name="Draft")
            editor.evaluate("el => { el.dataset.identity = 'original'; el.setSelectionRange(2, 4); }")
            for state in ("download", "unknown", "initialize", "listening", "finishing", "error", "idle"):
                page.locator(f'[data-fixture-state="{state}"]').click()
                measured = [page.locator(selector).bounding_box() for selector in selectors]
                for baseline, current in zip(initial, measured):
                    assert baseline and current
                    assert all(abs(baseline[key] - current[key]) < 0.5 for key in baseline), (width, theme, state, baseline, current)
                assert editor.get_attribute("data-identity") == "original"
                assert editor.input_value() == "The editable draft stays right here."
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth"), (width, theme, state, "horizontal overflow")
                if state == "listening":
                    page.screenshot(path=str(output / f"listening-{width}-{theme}.png"))
                if state == "download":
                    page.screenshot(path=str(output / f"download-{width}-{theme}.png"))
                results.append({"width": width, "theme": theme, "state": state, "rectangles": measured})
            assert not errors, errors
            page.close()
    browser.close()
(output / "measurements.json").write_text(json.dumps(results, indent=2))
print(f"PASS: {len(results)} state transitions; chat/composer/editor rectangles unchanged; no overflow or page errors")
