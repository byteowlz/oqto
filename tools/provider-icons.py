#!/usr/bin/env -S uv run --script
"""Pinned provider artwork and deterministic offline client exports. No package dependencies."""

import argparse
import hashlib
import json
import re
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACK = ROOT / "contracts/provider-icons"
REVISION = "9752d967494e48d8f83f2808feb2b0207bf70997"
UPSTREAM = "https://github.com/lobehub/lobe-icons"
BRANDS = [
    "ai21",
    "ai302",
    "aihubmix",
    "aimlapi",
    "alibaba",
    "alibabacloud",
    "anthropic",
    "anyscale",
    "arcee",
    "aws",
    "azure",
    "azureai",
    "baidu",
    "baseten",
    "bedrock",
    "bytedance",
    "cerebras",
    "chutes",
    "cloudflare",
    "cohere",
    "crusoe",
    "deepinfra",
    "deepseek",
    "elevenlabs",
    "fal",
    "featherless",
    "fireworks",
    "friendli",
    "githubcopilot",
    "google",
    "googlecloud",
    "groq",
    "huggingface",
    "hyperbolic",
    "inception",
    "inference",
    "ionet",
    "jina",
    "lambda",
    "leptonai",
    "liquid",
    "lmstudio",
    "meta",
    "minimax",
    "mistral",
    "moonshot",
    "morph",
    "nebius",
    "nousresearch",
    "novita",
    "nvidia",
    "ollama",
    "openai",
    "opencode",
    "openrouter",
    "parasail",
    "perplexity",
    "phala",
    "poe",
    "ppio",
    "qiniu",
    "replicate",
    "sambanova",
    "siliconcloud",
    "stepfun",
    "targon",
    "tavily",
    "tencentcloud",
    "together",
    "venice",
    "vercel",
    "vertexai",
    "vllm",
    "volcengine",
    "voyage",
    "workersai",
    "xai",
    "zai",
    "zhipu",
]
ALIASES = {
    "openai-codex": "openai",
    "amazon-bedrock": "bedrock",
    "azure-openai": "azure",
    "gemini": "google",
    "google-gemini-cli": "google",
    "google-antigravity": "google",
    "google-vertex": "vertexai",
    "google-vertex-anthropic": "vertexai",
    "grok": "xai",
    "together-ai": "together",
    "fireworks-ai": "fireworks",
    "lepton": "leptonai",
    "cloudflare-workers-ai": "workersai",
    "github-copilot": "githubcopilot",
    "moonshotai": "moonshot",
    "siliconflow": "siliconcloud",
    "z-ai": "zai",
    "featherless-ai": "featherless",
    "friendliai": "friendli",
    "io-net": "ionet",
    "nvidia-nim": "nvidia",
    "vercel-ai-gateway": "vercel",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return (
        json.dumps(value, indent=2, ensure_ascii=True, sort_keys=True) + "\n"
    ).encode()


def checked_svg(data):
    """Strict geometry-only SVG subset; not a general-purpose untrusted SVG sanitizer."""
    if len(data) > 128 * 1024 or b"<!" in data:
        raise ValueError("SVG size/declaration not allowed")
    root = ET.fromstring(data)
    namespace = "{http://www.w3.org/2000/svg}"
    tags = {
        "svg",
        "g",
        "path",
        "rect",
        "circle",
        "ellipse",
        "polygon",
        "polyline",
        "line",
        "defs",
        "clipPath",
        "mask",
        "title",
    }
    attributes = {
        "xmlns",
        "width",
        "height",
        "viewBox",
        "fill",
        "fill-rule",
        "clip-rule",
        "d",
        "opacity",
        "fill-opacity",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "stroke-miterlimit",
        "stroke-dasharray",
        "stroke-dashoffset",
        "stroke-opacity",
        "transform",
        "id",
        "clip-path",
        "mask",
        "mask-type",
        "x",
        "y",
        "x1",
        "x2",
        "y1",
        "y2",
        "cx",
        "cy",
        "r",
        "rx",
        "ry",
        "points",
        "clipPathUnits",
        "maskUnits",
        "maskContentUnits",
        "preserveAspectRatio",
        "color",
        "style",
    }
    if root.tag != namespace + "svg" or not re.fullmatch(
        r"0 0 [0-9.]+ [0-9.]+", root.attrib.get("viewBox", "")
    ):
        raise ValueError("SVG needs a bounded root viewBox")
    for node in root.iter():
        if node.tag.removeprefix(namespace) not in tags:
            raise ValueError(f"SVG element not allowed: {node.tag}")
        for name, value in node.attrib.items():
            if name not in attributes or any(
                token in value.lower()
                for token in ("http:", "https:", "data:", "javascript:", "//")
            ):
                raise ValueError(f"SVG attribute not allowed: {name}")
            if name == "style" and value != "flex:none;line-height:1":
                raise ValueError(
                    "Only the exact upstream non-paint root style is allowed"
                )
            if "url(" in value and not re.fullmatch(r"url\(#[A-Za-z0-9_-]+\)", value):
                raise ValueError("Only local geometry references allowed")
            if name in {"fill", "stroke", "color"} and value not in {
                "currentColor",
                "none",
                "black",
                "white",
                "#000",
                "#fff",
                "#000000",
                "#ffffff",
            }:
                raise ValueError("Only monochrome artwork allowed")
    return root.attrib["viewBox"]


def fetch(path):
    url = f"https://raw.githubusercontent.com/lobehub/lobe-icons/{REVISION}/{path}"
    with urllib.request.urlopen(url, timeout=30) as response:
        return response.read(128 * 1024 + 1)


def bootstrap():
    """Explicit upstream update; normal export/check never accesses the network."""
    from concurrent.futures import ThreadPoolExecutor

    paths = ["LICENSE"] + [f"packages/static-svg/icons/{brand}.svg" for brand in BRANDS]
    with ThreadPoolExecutor(max_workers=8) as pool:
        data = dict(zip(paths, pool.map(fetch, paths)))
    icons = {}
    outputs = {"LOBEHUB-LICENSE.txt": data["LICENSE"]}
    for brand in BRANDS:
        source = f"packages/static-svg/icons/{brand}.svg"
        svg = data[source]
        icons[brand] = {
            "path": f"svg/{brand}.svg",
            "sha256": digest(svg),
            "viewBox": checked_svg(svg),
            "source": source,
        }
        outputs[f"svg/{brand}.svg"] = svg
    aliases = {**{brand: brand for brand in BRANDS}, **ALIASES}
    catalog = {
        "version": 1,
        "style": "monochrome",
        "upstream": {
            "repository": UPSTREAM,
            "revision": REVISION,
            "license": "MIT",
            "license_sha256": digest(data["LICENSE"]),
        },
        "icons": icons,
        "aliases": aliases,
    }
    outputs["catalog.json"] = encoded(catalog)
    publish(PACK, outputs, False)


def load_pack():
    raw = (PACK / "catalog.json").read_bytes()
    catalog = json.loads(raw)
    if (
        catalog["version"] != 1
        or catalog["style"] != "monochrome"
        or catalog["upstream"]["revision"] != REVISION
    ):
        raise ValueError("Unsupported provider catalog")
    if (
        digest((PACK / "LOBEHUB-LICENSE.txt").read_bytes())
        != catalog["upstream"]["license_sha256"]
    ):
        raise ValueError("License drift")
    for brand, entry in catalog["icons"].items():
        if (
            not re.fullmatch(r"[a-z][a-z0-9]*", brand)
            or entry["path"] != f"svg/{brand}.svg"
        ):
            raise ValueError("Unsafe icon identity/path")
        svg = (PACK / entry["path"]).read_bytes()
        if digest(svg) != entry["sha256"] or checked_svg(svg) != entry["viewBox"]:
            raise ValueError(f"Artwork drift: {brand}")
    for alias, brand in catalog["aliases"].items():
        if not re.fullmatch(r"[a-z][a-z0-9-]*", alias) or brand not in catalog["icons"]:
            raise ValueError("Invalid provider alias")
    return catalog, raw


def rust_resolver(catalog):
    lines = [
        "// Generated by oqto tools/provider-icons.py; edit the canonical catalog, not this file.",
        "pub fn provider_icon_id(provider: &str) -> Option<&'static str> {",
        "    if !provider.is_ascii() {",
        "        return None;",
        "    }",
        "    match provider.trim().to_ascii_lowercase().as_str() {",
    ]
    for alias, brand in sorted(catalog["aliases"].items()):
        lines.append(f'        "{alias}" => Some("{brand}"),')
    return ("\n".join(lines + ["        _ => None,", "    }", "}", ""])).encode()


def export(platform, output, check):
    catalog, raw = load_pack()
    outputs = {
        "provider-icons-catalog.json": raw,
        "LOBEHUB-PROVIDER-LICENSE.txt": (PACK / "LOBEHUB-LICENSE.txt").read_bytes(),
    }
    if platform == "web":
        lines = ["// Generated from the canonical provider-icons catalog. Do not edit."]
        for brand in sorted(catalog["icons"]):
            lines.append(f'import {brand} from "./svg/{brand}.svg?raw";')
            outputs[f"svg/{brand}.svg"] = (PACK / f"svg/{brand}.svg").read_bytes()
        lines += [
            "function asDataUrl(svg: string): string {",
            "\treturn `data:image/svg+xml,${encodeURIComponent(svg)}`;",
            "}",
            "const icons: Record<string, string> = {",
        ]
        lines += [
            f"\t{brand}: asDataUrl({brand})," for brand in sorted(catalog["icons"])
        ]
        lines += ["};", "const aliases = new Map<string, string>(["]
        lines += [
            f'\t["{alias}", "{brand}"],'
            for alias, brand in sorted(catalog["aliases"].items())
        ]
        lines += [
            "]);",
            "export function providerIconUrl(provider: string): string | undefined {",
            "\tif ([...provider].some((char) => char.charCodeAt(0) > 127)) return undefined;",
            "\tconst id = aliases.get(provider.trim().toLowerCase());",
            "\treturn id ? icons[id] : undefined;",
            "}",
            "",
        ]
        outputs["provider-icons.ts"] = "\n".join(lines).encode()
    elif platform == "ios":
        for brand in catalog["icons"]:
            base = f"ProviderIcons.xcassets/ProviderIcon-{brand}.imageset"
            # Xcode compiles the silhouette; template mode supplies every theme's actual ink.
            outputs[f"{base}/{brand}.svg"] = (
                (PACK / f"svg/{brand}.svg")
                .read_bytes()
                .replace(b'"currentColor"', b'"#000000"')
            )
            outputs[f"{base}/Contents.json"] = encoded(
                {
                    "images": [{"filename": f"{brand}.svg", "idiom": "universal"}],
                    "info": {"author": "xcode", "version": 1},
                    "properties": {
                        "preserves-vector-representation": True,
                        "template-rendering-intent": "template",
                    },
                }
            )
        outputs["ProviderIcons.xcassets/Contents.json"] = encoded(
            {"info": {"author": "xcode", "version": 1}}
        )
    elif platform == "android":
        namespace = "http://schemas.android.com/apk/res/android"
        ET.register_namespace("android", namespace)
        for brand, entry in catalog["icons"].items():
            source = ET.fromstring((PACK / entry["path"]).read_bytes())
            _, _, width, height = entry["viewBox"].split()
            vector = ET.Element(
                "vector",
                {
                    f"{{{namespace}}}width": "24dp",
                    f"{{{namespace}}}height": "24dp",
                    f"{{{namespace}}}viewportWidth": width,
                    f"{{{namespace}}}viewportHeight": height,
                },
            )
            for node in source:
                kind = node.tag.split("}")[-1]
                if kind == "title":
                    continue
                if kind != "path":
                    raise ValueError(
                        "New Android geometry requires explicit conversion support"
                    )
                fill = node.attrib.get(
                    "fill", source.attrib.get("fill", "currentColor")
                )
                if fill == "none":
                    continue
                if fill != "currentColor":
                    raise ValueError(
                        "Android export requires the qualified template silhouette"
                    )
                attributes = {
                    f"{{{namespace}}}pathData": node.attrib["d"],
                    f"{{{namespace}}}fillColor": "#FF000000",
                }
                if (
                    node.attrib.get("fill-rule", source.attrib.get("fill-rule"))
                    == "evenodd"
                ):
                    attributes[f"{{{namespace}}}fillType"] = "evenOdd"
                if "fill-opacity" in node.attrib:
                    attributes[f"{{{namespace}}}fillAlpha"] = node.attrib[
                        "fill-opacity"
                    ]
                ET.SubElement(vector, "path", attributes)
            outputs[f"res/drawable/oqto_provider_{brand}.xml"] = (
                ET.tostring(vector, encoding="utf-8", xml_declaration=True) + b"\n"
            )
    elif platform == "rust":
        outputs["provider_icon_ids.rs"] = rust_resolver(catalog)
        table = [
            "// Generated bundled provider artwork. Do not edit.",
            "pub const PROVIDER_ICON_ASSETS: &[(&str, &[u8])] = &[",
        ]
        for brand in sorted(catalog["icons"]):
            table.append(
                f'    ("provider-icons/{brand}.svg", include_bytes!("svg/{brand}.svg")),'
            )
        outputs["provider_icon_assets.rs"] = ("\n".join(table + ["];", ""])).encode()
        for brand in catalog["icons"]:
            outputs[f"svg/{brand}.svg"] = (PACK / f"svg/{brand}.svg").read_bytes()
    else:
        raise ValueError("Unsupported export platform")
    outputs["PROVIDER-ICONS-SOURCE.json"] = encoded(
        {
            "version": 1,
            "catalog_sha256": digest(raw),
            "upstream": catalog["upstream"],
            "platform": platform,
            "files": {name: digest(data) for name, data in sorted(outputs.items())},
        }
    )
    guard_export(output, outputs, check)
    publish(output, outputs, check)


def guard_export(root, outputs, check):
    inventory = root / "PROVIDER-ICONS-SOURCE.json"
    if not inventory.exists():
        if root.exists() and any(root.iterdir()):
            raise ValueError("Refusing an unmanaged non-empty export directory")
        return
    previous = json.loads(inventory.read_bytes())["files"]
    for name, expected in previous.items():
        path = root / name
        if Path(name).is_absolute() or ".." in Path(name).parts or path.is_symlink():
            raise ValueError("Unsafe previous export path")
        if not path.is_file() or digest(path.read_bytes()) != expected:
            raise ValueError(f"Preserve modified generated file: {name}")
    for name in outputs:
        if (
            name not in previous
            and name != "PROVIDER-ICONS-SOURCE.json"
            and (root / name).exists()
        ):
            raise ValueError(f"Preserve unmanaged output collision: {name}")
    if not check:
        for name in previous.keys() - outputs.keys():
            (root / name).unlink()


def publish(root, outputs, check):
    for name, data in outputs.items():
        path = root / name
        if check:
            if not path.is_file() or path.read_bytes() != data:
                raise ValueError(f"Generated provider icon drift: {path}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    print(f"{'Verified' if check else 'Wrote'} {len(outputs)} provider-icon files")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("fetch", help="Fetch only the declared immutable upstream revision")
    sub.add_parser(
        "verify", help="Verify source artwork, geometry and license hashes offline"
    )
    export_parser = sub.add_parser("export", help="Deterministic offline client export")
    export_parser.add_argument(
        "--platform", choices=["web", "ios", "android", "rust"], required=True
    )
    export_parser.add_argument("--output", type=Path, required=True)
    export_parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.command == "fetch":
        bootstrap()
    elif args.command == "verify":
        catalog, _ = load_pack()
        print(
            f"Verified {len(catalog['icons'])} provider icons and {len(catalog['aliases'])} explicit aliases"
        )
    else:
        export(args.platform, args.output, args.check)


if __name__ == "__main__":
    main()
