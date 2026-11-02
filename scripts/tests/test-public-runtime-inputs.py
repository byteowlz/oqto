#!/usr/bin/env python3
"""Keep reviewed embedded product seeds distinct from private agent context."""

import shutil
import subprocess
import tempfile
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

    def test_generated_distribution_defaults_are_not_source_inputs(self) -> None:
        with tempfile.TemporaryDirectory(prefix="oqto-staging-policy-") as directory:
            fixture = Path(directory)
            shutil.copyfile(ROOT / ".gitignore", fixture / ".gitignore")
            subprocess.run(["git", "init", "-q", str(fixture)], check=True)
            for path, ignored in (
                ("dist/immutable/defaults/pi-agent/AGENTS.md", True),
                (
                    "dist/immutable/defaults/workdir-templates/default/settings.json",
                    True,
                ),
                ("dist/manifest.toml", False),
                ("scripts/dist/build-source-snapshot.py", False),
            ):
                with self.subTest(path=path):
                    target = fixture / path
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_text("synthetic fixture")
                    result = subprocess.run(
                        [
                            "git",
                            "-C",
                            str(fixture),
                            "-c",
                            "core.excludesFile=/dev/null",
                            "check-ignore",
                            "--no-index",
                            "--quiet",
                            path,
                        ],
                        check=False,
                    )
                    self.assertEqual(result.returncode, 0 if ignored else 1)

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
