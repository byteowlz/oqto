#!/usr/bin/env python3
"""Owner/workspace catalog isolation without reading a real owner profile."""
import tempfile
import unittest
from pathlib import Path
from profile_namespace import workspace_profile_key


class ProfileNamespaceTest(unittest.TestCase):
    def test_owner_isolation_and_canonical_retry(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            work = root / "workspace"
            owner_a = root / "owner a"
            owner_b = root / "owner b"
            for directory in (work, owner_a, owner_b):
                directory.mkdir()
            # Invalid JSON proves this is path identity, not configuration loading.
            (owner_a / "settings.json").write_text("not valid JSON")
            alias = root / "owner-alias"
            alias.symlink_to(owner_a, target_is_directory=True)
            first = workspace_profile_key(work, "coding", owner_a)
            self.assertEqual(first, workspace_profile_key(work / ".", "coding", alias))
            self.assertNotEqual(first, workspace_profile_key(work, "coding", owner_b))
            self.assertNotEqual(first, workspace_profile_key(work, "demo"))
            self.assertEqual(workspace_profile_key(work, "demo", owner_a), workspace_profile_key(work, "demo", owner_b))
            with self.assertRaises(ValueError):
                workspace_profile_key(work, "coding")
            with self.assertRaises(ValueError):
                workspace_profile_key(work, "coding", owner_a / "settings.json")


if __name__ == "__main__":
    unittest.main()
