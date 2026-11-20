# /// script
# dependencies = ["Pillow==13.0.0"]
# ///
"""Check actual browser glyph pixels against recorded DOM boxes and foreground/background ink."""

import argparse
import hashlib
import json
import re
from pathlib import Path

from PIL import Image


def rgb(value):
    match = re.fullmatch(r"rgb\((\d+),\s*(\d+),\s*(\d+)\)", value)
    if not match:
        raise ValueError("Expected opaque fixture RGB")
    return tuple(map(int, match.groups()))


def inspect(image, item, scale):
    box = item["box"]
    bounds = tuple(
        round(v * scale)
        for v in (box["x"], box["y"], box["x"] + box["width"], box["y"] + box["height"])
    )
    crop = image.crop(bounds)
    foreground, background = rgb(item["color"]), rgb(item["background"])
    vector = tuple(foreground[i] - background[i] for i in range(3))
    norm = sum(v * v for v in vector)
    if norm < 1:
        raise ValueError("Fixture foreground is indistinguishable from background")
    shape = bytearray()
    painted = tinted = 0
    for pixel in crop.get_flattened_data():
        alpha = sum((pixel[i] - background[i]) * vector[i] for i in range(3)) / norm
        ink = alpha > 0.1
        shape.append(int(ink))
        if ink:
            painted += 1
            tinted += int(
                max(
                    abs(pixel[i] - (background[i] + alpha * vector[i]))
                    for i in range(3)
                )
                < 4
            )
    if painted <= 10 or tinted / painted <= 0.95:
        raise ValueError(
            f"Incorrect or missing paint: {item['brand']}/{item['theme']}, pixels={painted}, tint={tinted}"
        )
    return {
        "brand": item["brand"],
        "theme": item["theme"],
        "painted_pixels": painted,
        "tinted_pixels": tinted,
        "shape_sha256": hashlib.sha256(shape).hexdigest(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("screenshot", type=Path)
    parser.add_argument("geometry", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    raw = json.loads(args.geometry.read_text())
    geometry = raw.get("data", {}).get("result", raw)
    if isinstance(geometry, str):
        geometry = json.loads(geometry)
    image = Image.open(args.screenshot).convert("RGB")
    scale = image.width / geometry["viewport"]
    if geometry["current"] != "openai":
        raise ValueError("The explicit offline provider switch did not settle")
    results = [inspect(image, item, scale) for item in geometry["icons"]]
    themes = {item["theme"] for item in results}
    for theme in themes:
        shapes = {item["shape_sha256"] for item in results if item["theme"] == theme}
        if len(shapes) < 70:
            raise ValueError(
                "A generic repeated shape cannot stand in for the real provider artwork"
            )
    args.output.write_text(
        json.dumps(
            {
                "oracle": "Antialiased RGB foreground/background line: alpha>0.1, <=4 channel residual; >10 painted pixels and >95% tint alignment per glyph; >=70 distinct shapes per theme",
                "screenshot_sha256": hashlib.sha256(
                    args.screenshot.read_bytes()
                ).hexdigest(),
                "offline_switch": geometry["current"],
                "icons": len(results),
                "results": results,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"PASS: {len(results)} actual browser glyphs paint the recorded theme ink")


if __name__ == "__main__":
    main()
