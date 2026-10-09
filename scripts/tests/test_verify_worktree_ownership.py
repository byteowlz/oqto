#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "dist" / "verify-worktree-ownership.py"
SPEC = importlib.util.spec_from_file_location("verify_worktree_ownership", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class VerifyWorktreeOwnershipTests(unittest.TestCase):
    def test_accepts_outputs_owned_by_effective_user(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "dist").mkdir()
            (root / "dist" / "owned.txt").write_text("ok")
            self.assertEqual(MODULE.ownership_errors(root, root.stat().st_uid), [])

    def test_rejects_root_execution_in_user_owned_checkout(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            if root.stat().st_uid == 0:
                self.skipTest("test requires a non-root-owned temporary directory")
            errors = MODULE.ownership_errors(root, 0)
            self.assertTrue(any("must not run as root" in error for error in errors))

    def test_reports_foreign_owned_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "dist" / "foreign.txt"
            output.parent.mkdir()
            output.write_text("blocked")
            errors = MODULE.ownership_errors(root, root.stat().st_uid + 1)
            self.assertTrue(any("foreign.txt" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
