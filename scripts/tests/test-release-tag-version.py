#!/usr/bin/env python3
"""Release tags must agree with Cargo and dependency versions before publishing."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
CHECK = ROOT / "scripts/release/check-tag-version.py"


class ReleaseTagTests(unittest.TestCase):
    def invoke(self, tag: str, cargo_version: str = "0.5.0", deps_version: str = "0.5.0"):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cargo = root / "Cargo.toml"
            deps = root / "dependencies.toml"
            cargo.write_text(f'[workspace.package]\nversion = "{cargo_version}"\n')
            deps.write_text(f'[oqto]\nversion = "{deps_version}"\n')
            return subprocess.run(
                [sys.executable, "-B", str(CHECK), "--tag", tag,
                 "--cargo", str(cargo), "--dependencies", str(deps)],
                text=True, capture_output=True, check=False,
            )

    def test_matching_tag_and_declared_versions_pass(self):
        self.assertEqual(self.invoke("v0.5.0").returncode, 0)
        self.assertEqual(self.invoke("v1.2.3-rc.1", "1.2.3-rc.1", "1.2.3-rc.1").returncode, 0)

    def test_branch_name_missing_or_wrong_version_fails(self):
        for tag in ("main", "latest", "", "v0.5.1", "v0.05.0", "v0.5.0\nunsafe"):
            self.assertNotEqual(self.invoke(tag).returncode, 0, tag)

    def test_dependency_pin_divergence_fails(self):
        self.assertNotEqual(self.invoke("v0.5.0", deps_version="0.4.9").returncode, 0)
        self.assertNotEqual(self.invoke("v0.5.0", cargo_version="0.4.9").returncode, 0)


if __name__ == "__main__":
    unittest.main()
