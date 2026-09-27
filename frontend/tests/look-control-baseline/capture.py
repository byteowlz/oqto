#!/usr/bin/env python3
"""Pin Chromium computed control styles from the committed Oqto Web CSS.

Run against the isolated Vite fixture, not the live Oqto frontend:
  python3 .../with_server.py --server 'bunx vite --host 127.0.0.1 --port 4273 --strictPort' --port 4273 -- python3 tests/look-control-baseline/capture.py --update
Repeat without --update to assert the pinned metrics; screenshots are review aids.
"""

import argparse
import hashlib
import json
from pathlib import Path

from playwright.sync_api import sync_playwright

FRONTEND = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
GOLDEN = HERE / "oqto-control-computed.json"
CONTROLS = {
    "shared-default": "#look-shared-default",
    "shared-outline": "#look-shared-outline",
    "shared-secondary-small": "#look-shared-secondary",
    "workbench-icon": "#look-workbench-icon",
}
SCHEMES = ("oqto-dark", "oqto-light")
STATES = ("default", "hover", "focus-visible", "active", "disabled")
STYLE_JS = """element => {
  const s = getComputedStyle(element);
  const rect = element.getBoundingClientRect();
  return {
    borderTopWidth: s.borderTopWidth, borderRightWidth: s.borderRightWidth,
    borderBottomWidth: s.borderBottomWidth, borderLeftWidth: s.borderLeftWidth,
    borderTopColor: s.borderTopColor, borderStyle: s.borderTopStyle,
    boxShadow: s.boxShadow, height: s.height, measuredHeight: `${rect.height}px`,
    paddingTop: s.paddingTop, paddingRight: s.paddingRight,
    paddingBottom: s.paddingBottom, paddingLeft: s.paddingLeft,
    outlineStyle: s.outlineStyle, outlineWidth: s.outlineWidth,
    outlineOffset: s.outlineOffset, outlineColor: s.outlineColor,
    backgroundColor: s.backgroundColor, color: s.color,
    borderRadius: s.borderTopLeftRadius, opacity: s.opacity,
    focusVisible: element.matches(':focus-visible'),
    active: element.matches(':active'), disabled: element.disabled,
  };
}"""


def settle(page):
    # The production Button uses transition-all; sample the settled value.
    page.wait_for_timeout(350)


def sample(page, selector, state):
    target = page.locator(selector)
    page.evaluate("document.activeElement?.blur()")
    page.mouse.move(1200, 660)
    target.evaluate("element => element.disabled = false")
    settle(page)
    if state == "hover":
        target.hover()
    elif state == "focus-visible":
        page.keyboard.press("Tab")
        for _ in range(16):
            if target.evaluate("element => element.matches(':focus-visible')"):
                break
            page.keyboard.press("Tab")
        else:
            raise AssertionError(f"keyboard focus never reached {selector}")
    elif state == "active":
        target.hover()
        box = target.bounding_box()
        if not box:
            raise AssertionError(f"missing bounding box for {selector}")
        page.mouse.move(box["x"] + box["width"] / 2, box["y"] + box["height"] / 2)
        page.mouse.down()
    elif state == "disabled":
        target.evaluate("element => element.disabled = true")
    settle(page)
    result = target.evaluate(STYLE_JS)
    if state == "focus-visible" and not result["focusVisible"]:
        raise AssertionError(f"{selector} was not :focus-visible")
    if state == "active" and not result["active"]:
        raise AssertionError(f"{selector} was not :active")
    if state == "disabled" and not result["disabled"]:
        raise AssertionError(f"{selector} was not disabled")
    if state == "active":
        page.mouse.up()
    target.evaluate("element => element.disabled = false")
    page.mouse.move(1200, 660)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default="http://127.0.0.1:4273")
    parser.add_argument("--update", action="store_true", help="write golden numbers and screenshots")
    args = parser.parse_args()
    sources = [
        "components/ui/button.tsx",
        "src/styles/globals.css",
        "src/oqto-ui/app/shell.css",
        "src/oqto-ui/platform/base24-theme.ts",
    ]
    source_hashes = {name: hashlib.sha256((FRONTEND / name).read_bytes()).hexdigest() for name in sources}
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1280, "height": 720}, device_scale_factor=1, reduced_motion="reduce")
        page.goto(f"{args.base_url}/look-control-baseline.html", wait_until="networkidle")
        page.locator("#look-workbench-icon").wait_for()
        page.evaluate("document.fonts.ready")
        measurements = {}
        for scheme in SCHEMES:
            page.evaluate("""async scheme => {
              const {applyOqtoUiScheme} = await import('/src/oqto-ui/platform/base24-theme.ts');
              applyOqtoUiScheme(document.documentElement, scheme);
            }""", scheme)
            settle(page)
            measurements[scheme] = {}
            for name, selector in CONTROLS.items():
                measurements[scheme][name] = {state: sample(page, selector, state) for state in STATES}
            if args.update:
                shots = HERE / "screenshots"
                shots.mkdir(exist_ok=True)
                page.screenshot(path=str(shots / f"{scheme}.png"))
        result = {
            "environment": {"browser": "Chromium", "version": browser.version, "viewport": [1280, 720], "dpr": 1},
            "sourceHashes": source_hashes,
            "measurements": measurements,
        }
        browser.close()
    if args.update:
        GOLDEN.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
        print(f"Pinned {len(SCHEMES)} schemes x {len(CONTROLS)} controls x {len(STATES)} states to {GOLDEN}")
    else:
        expected = json.loads(GOLDEN.read_text())
        if result["measurements"] != expected["measurements"]:
            for scheme in SCHEMES:
                for name in CONTROLS:
                    for state in STATES:
                        want = expected["measurements"][scheme][name][state]
                        got = result["measurements"][scheme][name][state]
                        if want != got:
                            changes = {key: [want.get(key), got.get(key)] for key in want.keys() | got.keys() if want.get(key) != got.get(key)}
                            raise AssertionError(f"{scheme}/{name}/{state} drift: {changes}")
        print(f"PASS: {len(SCHEMES)} schemes x {len(CONTROLS)} controls x {len(STATES)} states match the pinned computed styles")


if __name__ == "__main__":
    main()
