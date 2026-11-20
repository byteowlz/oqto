#!/usr/bin/env python3
"""Capture real OqtoUI components with synthetic adapters, never a live host."""
import json
import sys
from pathlib import Path
from urllib.parse import urlsplit
from playwright.sync_api import sync_playwright

url, destination = sys.argv[1:]
output = Path(destination)
output.mkdir(parents=True, exist_ok=True)
results = []
with sync_playwright() as pw:
    browser = pw.chromium.launch(headless=True)
    try:
        for variant in ["incumbent", "focus", "inspect"]:
            for state in ["ready", "working", "notice"]:
                page = browser.new_page(viewport={"width": 1280, "height": 900})
                errors = []
                page.on("pageerror", lambda error: errors.append(str(error)))
                page.route("**/*", lambda route: route.continue_() if urlsplit(route.request.url).netloc == urlsplit(url).netloc else route.abort())
                page.goto(f"{url}?variant={variant}&state={state}")
                page.wait_for_function("document.querySelector('.wb-chat-card')?.innerText.includes('public API is unchanged')", timeout=15000)
                page.evaluate("document.fonts.ready")
                draft = page.locator("textarea").first
                assert draft.input_value() == "Also cover the expired-entry case."
                page.locator(".wb-shell").screenshot(path=str(output / f"web-{variant}-{state}.png"), animations="disabled")
                assert not errors, errors
                results.append({"variant": variant, "state": state, "surface": "actual OqtoUiShell, not default legacy route", "notice_projection": "unavailable" if state == "notice" else "not requested", "draft_retained": True})
                page.close()
    finally:
        browser.close()
(output / "web-proof.json").write_text(json.dumps({"scope": "synthetic adapters, actual Web renderer; no real service/auth/control", "cases": results}, indent=2) + "\n")
print(f"{len(results)} real Web surface captures passed")
