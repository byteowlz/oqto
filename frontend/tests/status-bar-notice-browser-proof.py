#!/usr/bin/env python3
"""Bounded headless proof of production status chrome; no live account/Runner."""
import json
import os
import socket
import subprocess
import time
from pathlib import Path
from urllib.request import urlopen

from playwright.sync_api import sync_playwright

frontend = Path(__file__).resolve().parents[1]
output = frontend.parent / "target" / "web-status-notices"
output.mkdir(parents=True, exist_ok=True)
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
url = f"http://127.0.0.1:{port}/tests/status-bar-notice-fixture.html"
results = []
with (output / "vite.log").open("w") as log:
    server = subprocess.Popen(
        ["bun", "x", "--no-install", "vite", "--host", "127.0.0.1", "--port", str(port), "--strictPort"],
        cwd=frontend, stdout=log, stderr=subprocess.STDOUT,
        env={**os.environ, "BROWSER": "none"},
    )
    try:
        for _ in range(100):
            if server.poll() is not None:
                raise RuntimeError("Disposable Vite exited; see target/web-status-notices/vite.log")
            try:
                with urlopen(url, timeout=1) as response:
                    if response.status == 200:
                        break
            except OSError:
                time.sleep(0.1)
        else:
            raise TimeoutError("Disposable fixture origin did not start")
        with sync_playwright() as pw:
            browser = pw.chromium.launch(headless=True)
            try:
                for width, scale in [(320, 1), (390, 1), (768, 1), (1440, 1), (640, 2)]:
                    page = browser.new_page(viewport={"width": width, "height": 900})
                    errors = []
                    page.on("pageerror", lambda error: errors.append(str(error)))
                    page.goto(url)
                    page.get_by_role("textbox", name="Retained draft").wait_for()
                    page.evaluate("document.fonts.ready")
                    if scale != 1:
                        page.locator("html").evaluate("(node, scale) => node.style.fontSize = `${16 * scale}px`", scale)
                    page.locator('[data-proof="list"]').evaluate("node => node.scrollTop = 180")
                    def geometry():
                        return page.evaluate("""() => Object.fromEntries(
                            [...document.querySelectorAll('[data-proof]')].map(node => {
                                const {x,y,width,height} = node.getBoundingClientRect();
                                return [node.dataset.proof, {x,y,width,height,scrollTop:node.scrollTop}];
                            }))""")
                    baseline = geometry()
                    assert page.get_by_label("Recovery calls").inner_text() == "0"
                    page.get_by_role("button", name="Toggle catalog error").click()
                    trigger = page.get_by_role("button", name="Chat-Verlauf nicht erreichbar:")
                    trigger.wait_for()
                    assert geometry() == baseline, (width, "notice moved content")
                    assert page.get_by_label("Recovery calls").inner_text() == "0"
                    trigger.focus()
                    page.keyboard.press("Enter")
                    dialog = page.get_by_role("dialog", name="Chat-Verlauf nicht erreichbar")
                    dialog.wait_for()
                    assert geometry() == baseline, (width, "details moved content")
                    assert "Verbindungsunterbrechung" * 12 in dialog.inner_text()
                    assert dialog.evaluate("node => node.scrollWidth <= node.clientWidth"), "Details overflow horizontally"
                    box = dialog.bounding_box()
                    assert box and box["x"] >= 0 and box["x"] + box["width"] <= width, box
                    assert page.get_by_label("Recovery calls").inner_text() == "0"
                    dialog.evaluate("node => Promise.all(node.getAnimations({subtree:true}).map(animation => animation.finished))")
                    page.screenshot(path=str(output / f"{width}-{scale}x-details.png"))
                    page.keyboard.press("Escape")
                    dialog.wait_for(state="hidden")
                    assert trigger.evaluate("node => node === document.activeElement"), "Escape lost trigger focus"
                    assert page.get_by_label("Recovery calls").inner_text() == "0"
                    trigger.click()
                    page.get_by_role("button", name="Erneut versuchen").click()
                    assert page.get_by_label("Recovery calls").inner_text() == "1"
                    page.keyboard.press("Escape")
                    page.get_by_role("button", name="Toggle catalog error").click()
                    trigger.wait_for(state="detached")
                    assert geometry() == baseline, (width, "clear moved content")
                    assert page.get_by_role("textbox", name="Retained draft").input_value() == "Unsent fixture draft"
                    assert page.get_by_label("Recovery calls").inner_text() == "1"
                    assert not errors, errors
                    results.append({"width": width, "text_scale": scale, "geometry": baseline, "passed": True})
                    page.close()
            finally:
                browser.close()
        (output / "proof.json").write_text(json.dumps({"scope": "synthetic production status chrome, not full shell/native/live acceptance", "cases": results}, indent=2) + "\n")
        print(f"{len(results)} status-lane browser cases passed; {output}")
    finally:
        server.terminate()
        try:
            server.wait(timeout=10)
        except subprocess.TimeoutExpired:
            server.kill()
            server.wait(timeout=5)
