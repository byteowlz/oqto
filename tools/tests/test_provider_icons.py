"""Offline source/export conformance; synthetic hostile SVGs, never remote rendering."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "provider_icons", Path(__file__).parents[1] / "provider-icons.py"
)
ICONS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ICONS)


class ProviderIconsTest(unittest.TestCase):
    def test_pinned_catalog_license_geometry_and_aliases(self):
        catalog, _ = ICONS.load_pack()
        self.assertEqual(len(catalog["icons"]), 79)
        self.assertEqual(catalog["upstream"]["revision"], ICONS.REVISION)
        self.assertEqual(catalog["aliases"]["google-vertex-anthropic"], "vertexai")
        for unknown in [
            "custom-provider",
            "gateway/openai",
            "openai-compatible",
            "claude-opus",
            "__proto__",
        ]:
            self.assertNotIn(unknown, catalog["aliases"])

    def test_rejects_active_remote_coloured_and_oversized_svg(self):
        prefix = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">'
        for body in [
            "<script>alert(1)</script>",
            '<image href="https://example.invalid/track"/>',
            '<path d="M0 0" onclick="alert(1)"/>',
            "<foreignObject/>",
            '<path fill="red" d="M0 0"/>',
            '<path style="fill:red" d="M0 0"/>',
            '<path clip-path="url(https://example.invalid/track)" d="M0 0"/>',
            '<path style="background:url(data:x)" d="M0 0"/>',
        ]:
            with self.subTest(body=body), self.assertRaises(ValueError):
                ICONS.checked_svg((prefix + body + "</svg>").encode())
        for blob in [
            b"<!DOCTYPE svg>" + prefix.encode() + b"</svg>",
            b" " * (129 * 1024),
        ]:
            with self.assertRaises(ValueError):
                ICONS.checked_svg(blob)

    def test_each_offline_export_is_deterministic_hash_checked_and_preserves_local_edits(
        self,
    ):
        for platform in ["web", "ios", "android", "rust"]:
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as temp:
                root = Path(temp) / "export"
                ICONS.export(platform, root, False)
                ICONS.export(platform, root, True)
                first = (root / "PROVIDER-ICONS-SOURCE.json").read_bytes()
                ICONS.export(platform, root, False)
                self.assertEqual(
                    first, (root / "PROVIDER-ICONS-SOURCE.json").read_bytes()
                )
                inventory = json.loads(first)["files"]
                for name, expected in inventory.items():
                    self.assertEqual(ICONS.digest((root / name).read_bytes()), expected)
                owned = root / "provider-icons-catalog.json"
                owned.write_bytes(b"local edit must survive")
                for check in [True, False]:
                    with self.assertRaisesRegex(ValueError, "Preserve modified"):
                        ICONS.export(platform, root, check)
                self.assertEqual(owned.read_bytes(), b"local edit must survive")

    def test_refuses_an_unmanaged_output_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "notes.txt").write_text("not generated")
            with self.assertRaisesRegex(ValueError, "unmanaged"):
                ICONS.export("web", root, False)
            self.assertEqual((root / "notes.txt").read_text(), "not generated")


if __name__ == "__main__":
    unittest.main()
