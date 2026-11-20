"""Actual OqtoUI render/interaction proof; synthetic hosts, never a live Session."""

import hashlib
import json
import sys
from pathlib import Path
from urllib.parse import urlsplit

from playwright.sync_api import expect, sync_playwright

url, destination, fixture_path = sys.argv[1:]
output = Path(destination)
output.mkdir(parents=True, exist_ok=True)
fixture = json.loads(Path(fixture_path).read_text())
composition = (Path(__file__).parent / "fixtures/compact-image.svg").read_bytes()
caption = "The whole image stays in flow. Inspect the code and Markdown in Files without losing this draft."
raw_answer = "\n\n".join(
    [
        part["text"]
        for message in fixture["messages"]
        if message["role"] == "assistant"
        for part in message["parts"]
        if part["type"] == "text"
    ]
    + [caption]
)
raw_code = "export function fresh(age: number, ttl: number): boolean {\n  // Expired entries must not survive the boundary.\n  return age < ttl;\n}\n"
results = []


def shot(page, name):
    page.screenshot(path=str(output / f"{name}.png"), animations="disabled")


def route_fixture(route):
    target = urlsplit(route.request.url)
    if target.netloc != urlsplit(url).netloc:
        route.abort()
    elif target.path.startswith("/api/workspace/files/"):
        route.fulfill(status=200, content_type="image/svg+xml", body=composition)
    else:
        route.continue_()


with sync_playwright() as pw:
    browser = pw.chromium.launch(headless=True)
    try:
        context = browser.new_context(viewport={"width": 1280, "height": 900})
        page = context.new_page()
        page.route("**/*", route_fixture)
        page.goto(f"{url}?variant=incumbent&state=ready")
        expect(page.get_by_text("public API is unchanged", exact=False)).to_be_visible()
        page.evaluate("document.fonts.ready")
        shot(page, "web-after-ready")
        context.close()

        for scheme, width, height in [
            ("oqto-light", 1440, 1000),
            ("oqto-dark", 1440, 1000),
            ("oqto-light", 390, 844),
            ("oqto-dark", 320, 844),
        ]:
            context = browser.new_context(
                viewport={"width": width, "height": height},
                has_touch=width < 700,
                is_mobile=width < 700,
                permissions=["clipboard-read", "clipboard-write"],
            )
            page = context.new_page()
            errors = []
            page.on("pageerror", lambda error, errors=errors: errors.append(str(error)))
            page.route("**/*", route_fixture)
            page.goto(
                f"{url}?variant=incumbent&state=ready&scenario=media&scheme={scheme}"
            )
            image = page.get_by_alt_text("Synthetic full composition")
            expect(image).to_be_visible()
            page.wait_for_function(
                "document.querySelector('img[alt=\"Synthetic full composition\"]')?.naturalWidth === 800"
            )
            page.evaluate("document.fonts.ready")
            expect(page.locator("textarea").first).to_have_value(fixture["draft"])
            assert page.locator('[data-chat-presentation="compact"]').count() > 0
            dimensions = image.evaluate(
                "e => ({w:e.getBoundingClientRect().width,h:e.getBoundingClientRect().height, nw:e.naturalWidth,nh:e.naturalHeight,fit:getComputedStyle(e).objectFit,details:!!e.closest('details')})"
            )
            assert abs(dimensions["w"] / dimensions["h"] - 800 / 480) < 0.015, (
                dimensions
            )
            assert dimensions["fit"] == "contain" and not dimensions["details"], (
                dimensions
            )
            overflow = page.evaluate(
                "document.documentElement.scrollWidth > innerWidth + 1"
            )
            assert not overflow, {"width": width, "overflow": overflow}
            activity = page.locator("details.chat-activity").filter(
                has=page.get_by_text("1 failed", exact=True)
            )
            assert activity.count() == 1
            assert not activity.evaluate("e => e.open")
            summary = activity.locator(":scope > summary")
            typography = page.evaluate("""() => {
                const style = selector => getComputedStyle(document.querySelector(selector));
                return {body: style(".canonical-message-prose").fontFamily,
                    code: style(".canonical-message-prose code").fontFamily,
                    codeShadow: style(".canonical-message-prose code").textShadow,
                    accent: style(".chat-activity__status").fontFamily,
                    departureLoaded: [...document.fonts].some(f => f.family.includes("Departure") && f.status === "loaded"),
                    activity: style(".chat-activity > summary").backgroundColor,
                    canvas: style(".wb-chat-card").backgroundColor,
                    rowHeight: document.querySelector(".chat-activity > summary").getBoundingClientRect().height};
            }""")
            assert "JetBrains" in typography["body"], typography
            assert "JetBrains" in typography["code"], typography
            assert (
                "Departure" in typography["accent"] and typography["departureLoaded"]
            ), typography
            assert typography["codeShadow"] == "none", typography
            assert typography["activity"] not in [
                "transparent",
                "rgba(0, 0, 0, 0)",
                typography["canvas"],
            ], typography
            assert typography["rowHeight"] >= 44, typography
            image.scroll_into_view_if_needed()
            if width >= 700:
                page.locator(".wb-tree__name").filter(has_text="cache.ts").first.click()
                page.keyboard.press("Space")
                expect(
                    page.locator(".wb-files-preview .token.keyword").first
                ).to_be_visible()
                copy_code = page.locator(".wb-files-preview").get_by_role(
                    "button", name="Copy to clipboard"
                )
                copy_code.click()
                assert page.evaluate("navigator.clipboard.readText()") == raw_code
            shot(page, f"web-media-{scheme}-{width}")

            summary.focus()
            page.keyboard.press("Space")
            assert activity.evaluate("e => e.open")
            activity.get_by_role("button", name="Running task").click()
            expect(
                activity.get_by_text(
                    "Synthetic failure: expired entry was retained.", exact=True
                )
            ).to_be_visible()
            shot(page, f"web-expanded-{scheme}-{width}")
            summary.focus()
            page.keyboard.press("Space")
            assert not activity.evaluate("e => e.open")
            copy_answer = (
                page.locator(".chat-turn--agent .chat-byline")
                .get_by_role("button", name="Copy", exact=True)
                .last
            )
            copy_answer.focus()
            page.keyboard.press("Enter")
            assert page.evaluate("navigator.clipboard.readText()") == raw_answer
            if width < 700:
                box = copy_answer.bounding_box()
                assert box["width"] >= 44 and box["height"] >= 44, box
            else:
                page.locator(".wb-tree__name").filter(has_text="NOTES.md").first.click()
                expect(
                    page.locator(".wb-files-preview").get_by_role(
                        "heading", name="Cache boundary"
                    )
                ).to_be_visible()
                shot(page, f"web-markdown-{scheme}")
                page.locator(".wb-files-preview").get_by_role(
                    "button", name="Source", exact=True
                ).click()
                expect(page.locator(".wb-files-preview .token").first).to_be_visible()
                shot(page, f"web-markdown-source-{scheme}")
            expect(page.locator("textarea").first).to_have_value(fixture["draft"])
            assert not errors, errors
            results.append(
                {
                    "scheme": scheme,
                    "width": width,
                    "height": height,
                    "image": dimensions,
                    "page_overflow": overflow,
                    "typography_and_surface": typography,
                    "default_collapsed": True,
                    "keyboard_disclosure": True,
                    "failed_output_retained": True,
                    "exact_keyboard_copy": True,
                    "draft_retained": True,
                    "files_code_markdown": width >= 700,
                    "page_errors": errors,
                }
            )
            context.close()
    finally:
        browser.close()
files = [
    {"file": p.name, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
    for p in sorted(output.glob("*.png"))
]
(output / "proof.json").write_text(
    json.dumps(
        {
            "scope": "Actual OqtoUiShell/ChatPane/FileViewer, synthetic platform; no real host/auth/inference/delivery proof",
            "fixture_sha256": hashlib.sha256(
                Path(fixture_path).read_bytes()
            ).hexdigest(),
            "cases": results,
            "captures": files,
        },
        indent=2,
    )
    + "\n"
)
print(f"{len(results)} actual OqtoUI interaction cases, {len(files)} captures passed")
