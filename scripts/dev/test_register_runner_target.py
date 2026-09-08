import copy
from pathlib import Path
import tempfile
import tomllib
import unittest
from register_runner_target import register


class RegistrationTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "config.toml"
        self.original = b'# keep comments\n[backend]\nmode = "local"\n[unknown]\nsetting = "preserved"\n'
        self.path.write_bytes(self.original)
        self.target = {"id": "mac", "label": "Mac", "account_ids": ["alice"], "endpoint": {
            "transport": "tcp_tls", "address": "127.0.0.1:39443", "server_name": "mac.runner",
            "ca": "/private/ca.pem", "certificate": "/private/client.pem", "key": "/private/client-key.pem"}}

    def test_append_preserves_bytes_and_private_backup_and_is_idempotent(self):
        register(self.path, self.target)
        self.assertTrue(self.path.read_bytes().startswith(self.original))
        config = tomllib.loads(self.path.read_text())
        self.assertEqual(config["backend"]["mode"], "local")
        self.assertEqual(config["backend"]["runner"]["targets"], [self.target])
        backups = list(self.path.parent.glob("*.before-runner-target-*"))
        self.assertEqual(len(backups), 1)
        self.assertEqual(backups[0].read_bytes(), self.original)
        self.assertEqual(backups[0].stat().st_mode & 0o777, 0o600)
        self.assertEqual(register(self.path, self.target), "unchanged")
        self.assertEqual(len(list(self.path.parent.iterdir())), 2)

    def test_dry_run_and_conflicting_target_do_not_mutate(self):
        self.assertIn("[[backend.runner.targets]]", register(self.path, self.target, True))
        self.assertEqual(self.path.read_bytes(), self.original)
        register(self.path, self.target)
        before = self.path.read_bytes()
        other = copy.deepcopy(self.target)
        other["account_ids"] = ["bob"]
        with self.assertRaises(ValueError):
            register(self.path, other)
        self.assertEqual(self.path.read_bytes(), before)

    def test_additional_target_keeps_existing_grants(self):
        register(self.path, self.target)
        other = copy.deepcopy(self.target)
        other["id"] = "mac-two"
        other["account_ids"] = ["bob"]
        register(self.path, other)
        self.assertEqual(tomllib.loads(self.path.read_text())["backend"]["runner"]["targets"], [self.target, other])

    def test_invalid_and_non_tls_entries_fail_without_writes(self):
        for key, value in [("id", "../mac"), ("account_ids", []), ("endpoint", {"transport": "unix"})]:
            other = copy.deepcopy(self.target)
            other[key] = value
            with self.assertRaises(ValueError):
                register(self.path, other)
            self.assertEqual(self.path.read_bytes(), self.original)
        self.assertEqual(len(list(self.path.parent.iterdir())), 1)


if __name__ == "__main__":
    unittest.main()
