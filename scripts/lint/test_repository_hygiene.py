"""Synthetic index fixtures; never inspect or alter user runtime stores."""

import subprocess
import tempfile
import unittest
from pathlib import Path

from repository_hygiene import check, violations


class RepositoryHygieneTests(unittest.TestCase):
    def test_product_sources_fixtures_and_qualified_vendor_inputs_are_allowed(self):
        self.assertEqual(
            violations(
                [
                    "README.md",
                    "CLAUDE.md",
                    "publish.toml",
                    "frontend/bun.lock",
                    "backend/Cargo.lock",
                    "backend/src/target.rs",
                    "tools/oqt/dev.sh",
                    "scripts/dist/pi-node/package-lock.json",
                    "frontend/vendor/openui-lang-0.0.10.tgz",
                    "scripts/tests/fixtures/synthetic.log",
                    "dist/manifest.toml",
                    "dist/mutable-templates/etc/oqto/config.toml",
                    "dist/immutable/bin/.gitkeep",
                ]
            ),
            [],
        )

    def test_root_drift_is_rejected(self):
        for path in [
            "oqt",
            "package-lock.json",
            "quick-fix.sh",
            "new-probe/index.html",
        ]:
            with self.subTest(path=path):
                self.assertTrue(
                    any("unapproved root" in finding for finding in violations([path]))
                )

    def test_nested_caches_and_scratch_are_rejected(self):
        for path in [
            ".tmp/transcript.json",
            "scripts/lint/__pycache__/gate.cpython-314.pyc",
            "frontend/node_modules/package/index.js",
            "tools/.ruff_cache/index",
            "backend/.pytest_cache/state",
            "docs/.DS_Store",
            "tools/compiled.pyo",
        ]:
            with self.subTest(path=path):
                self.assertTrue(
                    any("cache or scratch" in finding for finding in violations([path]))
                )

    def test_generated_staging_is_rejected_without_banning_source_dist_directories(
        self,
    ):
        for path in [
            "target/debug/oqto",
            "backend/target/release/oqto",
            "frontend/dist/app.js",
            "dist/out/release.tar.gz",
            "dist/immutable/bin/oqto",
            "dist/immutable/frontend/app.js",
            "dist/immutable/defaults/settings.json",
            "dist/immutable/pi-runtime/pi/index.js",
            "dist/immutable/provider-login/app.js",
        ]:
            with self.subTest(path=path):
                self.assertTrue(
                    any(
                        "generated staging" in finding for finding in violations([path])
                    )
                )

    def test_real_index_includes_force_added_ignored_paths_not_local_cache(self):
        with tempfile.TemporaryDirectory(prefix="oqto-hygiene-") as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.run(
                    ["git", "-C", directory, *args], check=True, capture_output=True
                )

            git("init", "-q")
            (root / ".gitignore").write_text(".tmp/\n")
            (root / "README.md").write_text("# Synthetic fixture\n")
            git("add", ".gitignore", "README.md")
            scratch = root / ".tmp"
            scratch.mkdir()
            (scratch / "private.json").write_text("{}")
            self.assertEqual(check(root), [])
            git("add", "--force", ".tmp/private.json")
            self.assertTrue(
                any(".tmp/private.json" in finding for finding in check(root))
            )
            git("rm", "--cached", "--force", ".tmp/private.json")
            self.assertTrue((scratch / "private.json").exists())
            odd = root / "unexpected\nroot.sh"
            odd.write_text("fixture")
            git("add", odd.name)
            findings = check(root)
            self.assertEqual(len(findings), 1)
            self.assertIn("unexpected\\nroot.sh", findings[0])

    def test_missing_git_repository_fails_closed(self):
        with (
            tempfile.TemporaryDirectory(prefix="oqto-no-git-") as directory,
            self.assertRaises(subprocess.CalledProcessError),
        ):
            check(Path(directory))


if __name__ == "__main__":
    unittest.main()
