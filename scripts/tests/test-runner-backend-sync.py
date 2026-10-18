#!/usr/bin/env python3
import importlib.util
import pathlib
import subprocess
import sys
import tempfile
import unittest

SOURCE = pathlib.Path(__file__).resolve().parents[1] / "dev/check-runner-backend-sync.py"
spec = importlib.util.spec_from_file_location("backend_sync", SOURCE)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SyncTests(unittest.TestCase):
    def baseline(self):
        return {"main": "a" * 40, "head": "a" * 40, "main_backend": "b" * 40,
                "head_backend": "b" * 40, "backend_dirty": False}

    def test_equal_sources_pass(self):
        self.assertEqual(module.compare(self.baseline(), self.baseline()), [])

    def test_ui_branch_may_differ_without_backend_drift(self):
        remote = self.baseline()
        remote["head"] = "c" * 40
        self.assertEqual(module.compare(self.baseline(), remote), [])

    def test_each_kind_of_drift_fails(self):
        for key, value in [("main", "d" * 40), ("main_backend", "d" * 40),
                           ("head_backend", "d" * 40), ("backend_dirty", True)]:
            with self.subTest(key=key):
                remote = self.baseline()
                remote[key] = value
                self.assertTrue(module.compare(self.baseline(), remote))

    def test_real_probe_detects_untracked_backend_source_without_mutating_index(self):
        import json
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            subprocess.run(["git", "init", "-q", "--initial-branch=main", str(root)], check=True)
            (root / "backend").mkdir()
            (root / "backend/source.rs").write_text("// synthetic\n")
            subprocess.run(["git", "-C", str(root), "add", "backend"], check=True)
            subprocess.run(["git", "-C", str(root), "-c", "user.name=Fixture", "-c",
                            "user.email=fixture@example.invalid", "commit", "-qm", "fixture"], check=True)
            def probe():
                return json.loads(subprocess.check_output([sys.executable, "-c", module.PROBE, str(root)]))
            self.assertFalse(probe()["backend_dirty"])
            before = (root / ".git/index").read_bytes()
            (root / "backend/new.rs").write_text("// new fixture\n")
            self.assertTrue(probe()["backend_dirty"])
            self.assertEqual(before, (root / ".git/index").read_bytes())


if __name__ == "__main__":
    unittest.main()
