#!/usr/bin/env python3

import importlib.util
from pathlib import Path
import tomllib
import unittest

SCRIPT = Path(__file__).parents[1] / "dist" / "merge-sandbox-deny.py"
SPEC = importlib.util.spec_from_file_location("merge_sandbox_deny", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

REQUIRED = ["/run/user", "/run/oqto/runner-sockets"]


class MergeSandboxDenyTests(unittest.TestCase):
    def test_extends_existing_multiline_override_without_losing_entries(self):
        source = 'enabled = true\ndeny_read = [\n    "~/.ssh",\n]\n\n[ssh]\nenabled = true\n'
        merged = MODULE.merge_deny_read(source, REQUIRED)
        parsed = tomllib.loads(merged)
        self.assertEqual(parsed["deny_read"], ["~/.ssh", *REQUIRED])
        self.assertIn("[ssh]\nenabled = true", merged)

    def test_extends_existing_inline_override(self):
        source = 'deny_read = ["~/.ssh"]\n[ssh]\nenabled = true\n'
        merged = MODULE.merge_deny_read(source, REQUIRED)
        self.assertEqual(tomllib.loads(merged)["deny_read"], ["~/.ssh", *REQUIRED])

    def test_inserts_override_before_first_table(self):
        source = 'enabled = true\n\n[ssh]\nenabled = true\n'
        merged = MODULE.merge_deny_read(source, REQUIRED)
        self.assertEqual(tomllib.loads(merged)["deny_read"], REQUIRED)
        self.assertLess(merged.index("deny_read"), merged.index("[ssh]"))

    def test_is_idempotent(self):
        source = 'deny_read = ["/run/user", "/run/oqto/runner-sockets"]\n'
        self.assertEqual(MODULE.merge_deny_read(source, REQUIRED), source)


if __name__ == "__main__":
    unittest.main()
