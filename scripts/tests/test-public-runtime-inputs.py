#!/usr/bin/env python3
"""Keep reviewed embedded product seeds distinct from private agent context."""

import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TEMPLATES = ROOT / "backend/crates/oqto/src/templates/embedded"
SEEDS = ("agent-instructions.md", "BOOTSTRAP.md", "PERSONALITY.md", "USER.md")


class RuntimeInputsTests(unittest.TestCase):
    def test_product_seeds_are_present_in_source_and_export(self) -> None:
        for name in SEEDS:
            with self.subTest(name=name):
                self.assertTrue((TEMPLATES / name).read_text().strip())
        self.assertFalse((TEMPLATES / "AGENTS.md").exists())

    def test_private_manifest_explicitly_allows_runtime_seeds(self) -> None:
        manifest = ROOT / "publish.toml"
        if not manifest.exists():
            # Public exports intentionally omit publication policy; presence is
            # checked above and normal Rust compilation verifies include_str!.
            self.skipTest(
                "public exports intentionally omit the private publication policy"
            )
        policy = tomllib.loads(manifest.read_text())
        for name in SEEDS:
            with self.subTest(name=name):
                self.assertIn(str((TEMPLATES / name).relative_to(ROOT)), policy["docs"])
        self.assertNotIn("AGENTS.md", policy["docs"])
        self.assertNotIn("CONTEXT.md", policy["docs"])


if __name__ == "__main__":
    unittest.main()
