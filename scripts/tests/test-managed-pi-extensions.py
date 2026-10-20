#!/usr/bin/env python3
"""Isolated producer migration/rollback contract; no real homes or secrets."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "managed", ROOT / "scripts/dist/managed-pi-extensions.py"
)
MANAGED = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MANAGED)


def sha(value):
    return hashlib.sha256(value.encode()).hexdigest()


class Migration(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / "home"
        self.home.mkdir()
        self.src = self.root / "source"
        self.dest = self.home / ".pi/agent/extensions"
        self.dest.mkdir(parents=True)
        self.write(self.src / "pi-todolist/index.ts", "current")
        self.write(self.dest / "pi-oqto-todos/index.ts", "old")
        self.write(self.home / ".pi/agent/tasks/state.json", "private task data")
        self.write(
            self.home / ".pi/agent/settings.json",
            '{"packages":["preserve-user-package"]}',
        )
        self.policy = {
            "version": 1,
            "supersessions": {
                "pi-oqto-todos": {
                    "replacement": "pi-todolist",
                    "index_sha256": [sha("old")],
                }
            },
            "managed_index_sha256": {"pi-todolist": [sha("previous")]},
        }

    def write(self, path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value)

    def run_sync(self):
        return MANAGED.reconcile(self.home, self.src, ["pi-todolist"], self.policy)

    def test_known_retirement_preserves_bytes_owner_tasks_settings_and_unknown_extension(
        self,
    ):
        old = self.dest / "pi-oqto-todos/index.ts"
        old.chmod(0o640)
        before = old.stat()
        self.write(self.dest / "my-extension/index.ts", "custom")
        result = self.run_sync()
        archived = Path(result["backup"]) / "pi-oqto-todos/index.ts"
        self.assertEqual(archived.read_text(), "old")
        self.assertEqual(
            (archived.stat().st_uid, archived.stat().st_gid, archived.stat().st_mode),
            (before.st_uid, before.st_gid, before.st_mode),
        )
        self.assertFalse(old.exists())
        self.assertEqual((self.dest / "my-extension/index.ts").read_text(), "custom")
        self.assertEqual(
            (self.home / ".pi/agent/tasks/state.json").read_text(), "private task data"
        )
        self.assertEqual(
            (self.home / ".pi/agent/settings.json").read_text(),
            '{"packages":["preserve-user-package"]}',
        )

    def test_second_run_is_noop_including_mtime_and_archive_count(self):
        self.run_sync()
        index = self.dest / "pi-todolist/index.ts"
        before = index.stat().st_mtime_ns
        archives = list((self.home / ".pi/agent/extension-backups").iterdir())
        result = self.run_sync()
        self.assertFalse(result["changed"])
        self.assertEqual(index.stat().st_mtime_ns, before)
        self.assertEqual(
            list((self.home / ".pi/agent/extension-backups").iterdir()), archives
        )

    def test_unknown_retired_name_is_preserved_not_assumed_managed(self):
        self.write(self.dest / "pi-oqto-todos/index.ts", "user-owned unknown code")
        result = self.run_sync()
        self.assertEqual(result["unknown_preserved"], ["pi-oqto-todos"])
        self.assertEqual(
            (self.dest / "pi-oqto-todos/index.ts").read_text(),
            "user-owned unknown code",
        )

    def test_unknown_canonical_index_fails_without_retiring_anything(self):
        self.write(self.dest / "pi-todolist/index.ts", "user-owned unknown code")
        with self.assertRaisesRegex(ValueError, "unknown user-owned"):
            self.run_sync()
        self.assertEqual((self.dest / "pi-oqto-todos/index.ts").read_text(), "old")
        self.assertEqual(
            (self.dest / "pi-todolist/index.ts").read_text(), "user-owned unknown code"
        )

    def test_missing_required_source_does_not_retire_existing_assets(self):
        (self.src / "pi-todolist/index.ts").unlink()
        with self.assertRaisesRegex(ValueError, "missing from source"):
            self.run_sync()
        self.assertTrue((self.dest / "pi-oqto-todos/index.ts").is_file())

    def test_copy_failure_keeps_old_paths(self):
        with patch.object(
            MANAGED.shutil, "copytree", side_effect=OSError("copy failed")
        ):
            with self.assertRaisesRegex(OSError, "copy failed"):
                self.run_sync()
        self.assertTrue((self.dest / "pi-oqto-todos/index.ts").is_file())
        self.assertFalse(list((self.home / ".pi/agent").glob(".extension-stage-*")))

    def test_second_archive_failure_rolls_back_every_prior_archive(self):
        self.write(self.dest / "pi-todolist/index.ts", "previous")
        rename = MANAGED.os.rename
        calls = 0

        def fail_second(src, dst):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise OSError("second archive failed")
            return rename(src, dst)

        with patch.object(MANAGED.os, "rename", side_effect=fail_second):
            with self.assertRaisesRegex(OSError, "second archive"):
                self.run_sync()
        self.assertEqual((self.dest / "pi-oqto-todos/index.ts").read_text(), "old")
        self.assertEqual((self.dest / "pi-todolist/index.ts").read_text(), "previous")

    def test_activation_failure_restores_both_old_assets(self):
        self.write(self.dest / "pi-todolist/index.ts", "previous")
        rename = MANAGED.os.rename
        calls = 0

        def fail_activation(src, dst):
            nonlocal calls
            calls += 1
            if calls == 3:
                raise OSError("activation failed")
            return rename(src, dst)

        with patch.object(MANAGED.os, "rename", side_effect=fail_activation):
            with self.assertRaisesRegex(OSError, "activation"):
                self.run_sync()
        self.assertEqual((self.dest / "pi-oqto-todos/index.ts").read_text(), "old")
        self.assertEqual((self.dest / "pi-todolist/index.ts").read_text(), "previous")

    def test_other_home_is_not_modified(self):
        other = self.root / "other-home/.pi/agent/extensions/pi-oqto-todos/index.ts"
        self.write(other, "other principal")
        self.run_sync()
        self.assertEqual(other.read_text(), "other principal")

    def test_foreign_uid_is_refused(self):
        with patch.object(MANAGED.os, "geteuid", return_value=os.geteuid() + 1):
            with self.assertRaisesRegex(PermissionError, "home owner"):
                self.run_sync()
        self.assertTrue((self.dest / "pi-oqto-todos/index.ts").is_file())

    def test_symlinked_destination_is_refused(self):
        other = self.root / "other"
        self.dest.rename(other)
        self.dest.symlink_to(other, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlinked managed"):
            self.run_sync()
        self.assertTrue((other / "pi-oqto-todos/index.ts").is_file())

    def test_backup_symlink_cannot_route_archives_back_into_autoload(self):
        (self.home / ".pi/agent/extension-backups").symlink_to(
            self.dest, target_is_directory=True
        )
        with self.assertRaisesRegex(ValueError, "symlinked managed"):
            self.run_sync()
        self.assertTrue((self.dest / "pi-oqto-todos/index.ts").is_file())

    def test_lock_symlink_is_not_followed(self):
        target = self.home / "untouched"
        target.write_text("user data")
        (self.home / ".pi/agent/.managed-extension-sync.lock").symlink_to(target)
        with self.assertRaises(OSError):
            self.run_sync()
        self.assertEqual(target.read_text(), "user data")
        self.assertTrue((self.dest / "pi-oqto-todos/index.ts").is_file())

    def test_real_policy_records_both_observed_todo_assets(self):
        policy = json.loads(
            (ROOT / "scripts/dist/pi-extension-supersessions.json").read_text()
        )
        rules = policy["supersessions"]
        self.assertEqual(rules["pi-oqto-todos"]["replacement"], "pi-todolist")
        self.assertIn(
            "30dc9d04507ad31b72938770cf6ffbf955a92adf502fff67a620975d80f1c3cb",
            rules["pi-oqto-todos"]["index_sha256"],
        )
        self.assertIn(
            "615142736af6515843b63721d218509239f10a9b7c064fb2e5751f069ce2b5dd",
            rules["oqto-todos"]["index_sha256"],
        )


if __name__ == "__main__":
    unittest.main()
