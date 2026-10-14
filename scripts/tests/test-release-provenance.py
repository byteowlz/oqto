#!/usr/bin/env python3
"""Packaging records the declared version and source commit, or refuses."""

from __future__ import annotations

import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

SCRIPT = (
    Path(__file__).resolve().parents[2] / "scripts/dist/record-release-provenance.py"
)
MANIFEST = 'manifest_version = 1\nid = "oqto-dist"\n\n[release]\ntarget = "full"\npi_version = "1.1.0"\n\n[targets.runner_only]\nplatform = "linux"\n'


def git(root: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(root), *args], check=True, capture_output=True, text=True
    ).stdout


class Provenance(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "repo"
        (self.root / "backend").mkdir(parents=True)
        (self.root / "backend/Cargo.toml").write_text(
            '[workspace.package]\nversion = "0.5.9"\n'
        )
        git(self.root, "init", "-q")
        git(
            self.root,
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "user.name=t",
            "add",
            ".",
        )
        git(
            self.root,
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        )
        self.commit = git(self.root, "rev-parse", "HEAD").strip()
        self.manifest = Path(self.tmp.name) / "manifest.toml"
        self.manifest.write_text(MANIFEST)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def run_script(self, version: str, *extra: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                "-B",
                str(SCRIPT),
                "--root",
                str(self.root),
                "--manifest",
                str(self.manifest),
                "--version",
                version,
                *extra,
            ],
            capture_output=True,
            text=True,
        )

    def release(self) -> dict:
        return tomllib.loads(self.manifest.read_text())["release"]

    def test_clean_declared_version_is_recorded_without_disturbing_other_tables(
        self,
    ) -> None:
        for requested in ("0.5.9", "v0.5.9"):
            self.manifest.write_text(MANIFEST)
            result = self.run_script(requested)
            self.assertEqual(result.returncode, 0, result.stderr)
            release = self.release()
            self.assertEqual(
                (release["version"], release["source_commit"], release["source_dirty"]),
                ("0.5.9", self.commit, False),
            )
            self.assertEqual(release["pi_version"], "1.1.0")
            self.assertEqual(
                tomllib.loads(self.manifest.read_text())["targets"]["runner_only"][
                    "platform"
                ],
                "linux",
            )

    def test_undeclared_or_nonsemantic_versions_are_refused(self) -> None:
        for requested in ("0.5.8", "0.6.0", "dev", "20261009120000", "local"):
            result = self.run_script(requested)
            self.assertNotEqual(result.returncode, 0, requested)
            self.assertEqual(
                self.manifest.read_text(),
                MANIFEST,
                f"{requested} modified the manifest",
            )

    def test_dirty_or_untracked_worktree_is_refused_unless_marked(self) -> None:
        for dirt in ("tracked", "untracked"):
            self.manifest.write_text(MANIFEST)
            if dirt == "tracked":
                (self.root / "backend/Cargo.toml").write_text(
                    '[workspace.package]\nversion = "0.5.9"\n# edit\n'
                )
            else:
                git(self.root, "checkout", "--", "backend/Cargo.toml")
                (self.root / "scratch.txt").write_text("uncommitted")
            refused = self.run_script("0.5.9")
            self.assertNotEqual(refused.returncode, 0, dirt)
            self.assertIn("uncommitted", refused.stderr)
            self.assertEqual(self.manifest.read_text(), MANIFEST)
            marked = self.run_script("0.5.9", "--allow-dirty")
            self.assertEqual(marked.returncode, 0, marked.stderr)
            self.assertIs(self.release()["source_dirty"], True)

    def test_existing_provenance_is_never_overwritten(self) -> None:
        self.manifest.write_text(
            MANIFEST.replace(
                'target = "full"\n', 'target = "full"\nversion = "9.9.9"\n'
            )
        )
        result = self.run_script("0.5.9")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("already declares", result.stderr)

    def head8(self) -> str:
        return self.commit[:8]

    def test_dev_versions_describe_their_source_truthfully(self) -> None:
        clean = f"0.5.9-dev.20261014061200.g{self.head8()}"
        result = self.run_script(clean)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.release()["version"], clean)
        self.assertIs(self.release()["source_dirty"], False)

        (self.root / "scratch.txt").write_text("work in progress")
        for wrong, reason in (
            (clean, "omits .dirty"),
            (f"0.5.8-dev.20261014061200.g{self.head8()}.dirty", "does not match"),
            ("0.5.9-dev.20261014061200.gdeadbeef.dirty", "names commit"),
        ):
            self.manifest.write_text(MANIFEST)
            refused = self.run_script(wrong)
            self.assertNotEqual(refused.returncode, 0, wrong)
            self.assertIn(reason, refused.stderr)
        self.manifest.write_text(MANIFEST)
        dirty = f"{clean}.dirty"
        accepted = self.run_script(dirty)
        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        self.assertIs(self.release()["source_dirty"], True)
        # A dirty tree still never yields a plain release version.
        self.manifest.write_text(MANIFEST)
        self.assertNotEqual(self.run_script("0.5.9").returncode, 0)


if __name__ == "__main__":
    unittest.main()
