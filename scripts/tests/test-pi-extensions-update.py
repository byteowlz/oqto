#!/usr/bin/env python3
"""Unit tests for scripts/release/pi-extensions-update.py (no network, no Pi)."""

import hashlib
import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "pi_extensions_update", ROOT / "scripts/release/pi-extensions-update.py"
)
UPDATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(UPDATE)


def sha(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


class Repo:
    def __init__(self, root: Path):
        self.root = root
        root.mkdir()
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "t@example.invalid")
        self.git("config", "user.name", "t")

    def git(self, *args: str) -> str:
        return subprocess.run(
            ["git", "-C", str(self.root), *args],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

    def commit(self, files: dict[str, str]) -> str:
        for name, text in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "c")
        return self.git("rev-parse", "HEAD")


class PiExtensionsUpdate(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.tmp = Path(temp.name)
        self.repo = Repo(self.tmp / "repo")
        self.first = self.repo.commit({"pi-a/index.ts": "v1", "pi-b/index.ts": "b1"})
        self.second = self.repo.commit({"pi-a/index.ts": "v2"})
        self.head = self.repo.commit({"pi-a/index.ts": "v3", "pi-b/x.ts": "x"})

    def test_history_digests_cover_every_committed_version(self):
        found = UPDATE.history_digests(self.repo.root, self.head, "pi-a")
        self.assertEqual(found, {sha("v1"), sha("v2"), sha("v3")})
        # Not reachable from an older revision.
        self.assertNotIn(
            sha("v3"), UPDATE.history_digests(self.repo.root, self.second, "pi-a")
        )

    def test_merged_policy_never_drops_known_digests(self):
        policy = {
            "version": 1,
            "source_revision": "old",
            "supersessions": {"a": {"replacement": "pi-a", "index_sha256": ["s"]}},
            "managed_index_sha256": {"pi-a": ["legacy"], "pi-z": ["keep"]},
        }
        merged = UPDATE.merged_policy(
            policy, self.repo.root, self.head, ["pi-a", "pi-b"]
        )
        self.assertEqual(merged["source_revision"], self.head)
        self.assertIn("legacy", merged["managed_index_sha256"]["pi-a"])
        self.assertIn(sha("v1"), merged["managed_index_sha256"]["pi-a"])
        self.assertEqual(merged["managed_index_sha256"]["pi-b"], [sha("b1")])
        self.assertEqual(merged["managed_index_sha256"]["pi-z"], ["keep"])
        self.assertEqual(merged["supersessions"], policy["supersessions"])
        self.assertEqual(policy["source_revision"], "old")  # input untouched

    def test_rewrite_pin_replaces_only_the_extension_pin(self):
        text = (
            '[external]\npi = "1.4.0"\n'
            f'pi-extensions = "{"a" * 40}"  # old note\nnode = "24"\n'
        )
        out = UPDATE.rewrite_pin(text, "b" * 40, "qualified")
        self.assertIn(f'pi-extensions = "{"b" * 40}"  # qualified', out)
        self.assertIn('pi = "1.4.0"', out)
        self.assertIn('node = "24"', out)
        with self.assertRaises(ValueError):
            UPDATE.rewrite_pin("[external]\n", "b" * 40, "x")

    def qualify(self, pin: str, ref: str = "HEAD", names=("pi-a", "pi-b")):
        manifest = self.tmp / "dependencies.toml"
        manifest.write_text(f'[external]\npi = "1.4.0"\npi-extensions = "{pin}"\n')
        policy = self.tmp / "policy.json"
        policy.write_text(json.dumps({"version": 1, "managed_index_sha256": {}}))
        pi = self.tmp / "pi"
        pi.write_text("")
        gates = []

        def fake_gate(name, argv, cwd, timeout, log_dir):
            gates.append(name)
            return {"gate": name, "seconds": 0}

        args = SimpleNamespace(
            repo=str(self.repo.root),
            ref=ref,
            pi=str(pi),
            pi_version=None,
            skip_unit=False,
            keep_work=False,
        )
        with (
            patch.object(UPDATE, "MANIFEST", manifest),
            patch.object(UPDATE, "POLICY", policy),
            patch.object(UPDATE, "default_extensions", return_value=list(names)),
            patch.object(UPDATE, "run_gate", side_effect=fake_gate),
        ):
            return UPDATE.qualify(args), gates

    def test_qualify_runs_every_gate_for_a_forward_candidate(self):
        receipt, gates = self.qualify(self.first)
        self.assertEqual(receipt["candidate"], self.head)
        self.assertEqual(gates, ["install", "unit", "load"])
        self.assertEqual(receipt["pi"]["version"], "1.4.0")

    def test_candidate_that_does_not_contain_the_pin_is_refused(self):
        self.repo.git("checkout", "-q", "-b", "side", self.first)
        side = self.repo.commit({"pi-a/index.ts": "side"})
        self.repo.git("checkout", "-q", "main")
        with self.assertRaisesRegex(UPDATE.GateFailed, "ancestry"):
            self.qualify(side, ref="main")

    def test_missing_required_extension_is_refused(self):
        with self.assertRaisesRegex(UPDATE.GateFailed, "pi-missing"):
            self.qualify(self.first, names=("pi-a", "pi-missing"))


if __name__ == "__main__":
    unittest.main()
