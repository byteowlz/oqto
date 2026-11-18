#!/usr/bin/env python3
"""Synthetic tests for the consumer verifier and the drift classifier.

Every fixture is a disposable copy of the real bundle; nothing touches a live
consumer checkout. The tamper cases assert that a rehashed bundle is refused.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

import runtime_consumer_verify as verify
import runtime_interactions_contract as core
import runtime_consumer_drift as drift

ROOT = core.ROOT
CORE_BUNDLE = ROOT / core.BUNDLE
VENDOR = Path("runtime-interactions/protocol/runtime-interactions")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def make_core(destination: Path) -> tuple[Path, str]:
    """Copy the current bundle and its inputs into a throwaway Core repository."""
    bundle = destination / core.BUNDLE
    shutil.copytree(CORE_BUNDLE, bundle)
    manifest = json.loads((bundle / "manifest.json").read_bytes())
    for relative in [*manifest["inputs"], "LICENSE"]:
        target = destination / relative
        if target.exists():
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / relative, target)
    subprocess.run(["git", "init", "-q", str(destination)], check=True)
    subprocess.run(["git", "-C", str(destination), "add", "-A"], check=True)
    subprocess.run(
        [
            "git",
            "-C",
            str(destination),
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "core snapshot",
        ],
        check=True,
    )
    commit = subprocess.run(
        ["git", "-C", str(destination), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    return destination, commit


class Consumer(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="rt-consumer-")
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name) / "consumer"
        self.bundle = self.repo / VENDOR / "v1"
        shutil.copytree(CORE_BUNDLE, self.bundle)
        self.core_root, self.core_ref = make_core(Path(self.temp.name) / "core")
        self.source_path = self.bundle.parent / "SOURCE.json"
        manifest = json.loads((self.bundle / "manifest.json").read_bytes())
        self.manifest = manifest
        self.write_source()

    def write_source(self, **overrides):
        manifest_bytes = (self.bundle / "manifest.json").read_bytes()
        manifest = json.loads(manifest_bytes)
        source = {
            "contract": "oqto.runtime-interactions",
            "version": 1,
            "core_source": "oqto",
            "core_commit": overrides.pop("core_commit", self.core_ref),
            "core_path": core.BUNDLE.as_posix(),
            "manifest_sha256": overrides.pop("manifest_sha256", sha256(manifest_bytes)),
            "file_count": len(manifest["files"]) + 1,
            "input_count": len(manifest["inputs"]),
            "vendored_to": (VENDOR / "v1").as_posix(),
        }
        source.update(overrides)
        self.source_path.write_text(json.dumps(source, indent=1) + "\n")

    def rewrite_manifest(self):
        """Recompute every hash, as an editor of a copy would."""
        manifest_path = self.bundle / "manifest.json"
        manifest = json.loads(manifest_path.read_bytes())
        manifest["files"] = {
            name: sha256((self.bundle / name).read_bytes()) for name in manifest["files"]
        }
        manifest_path.write_text(json.dumps(manifest, indent=1) + "\n")

    def assertDrift(self, pattern: str):
        with self.assertRaisesRegex(verify.Drift, pattern):
            verify.verify(self.bundle, self.source_path, self.core_root)

    def test_current_pinned_consumer_passes(self):
        receipt = verify.verify(self.bundle, self.source_path, self.core_root)
        self.assertEqual(receipt["core_commit"], self.core_ref)
        self.assertEqual(receipt["files"], 68)
        self.assertTrue(receipt["core_bytes_verified"])
        self.assertEqual(receipt["counts"]["request"], 11)

    def test_pin_only_mode_skips_commit_comparison(self):
        receipt = verify.verify(self.bundle, self.source_path, self.core_root, skip_core=True)
        self.assertFalse(receipt["core_bytes_verified"])

    def test_edited_file_is_refused(self):
        target = self.bundle / "RuntimeRequest.ts"
        target.write_text("export type RuntimeRequest = any;\n")
        self.assertDrift("hash mismatch")

    def test_regenerated_manifest_is_still_refused(self):
        target = self.bundle / "RuntimeRequest.ts"
        target.write_text("export type RuntimeRequest = any;\n")
        self.rewrite_manifest()
        # Manifest now self-consistent, but it is not the pinned manifest.
        self.assertDrift("is not the pinned")

    def test_rehashed_pin_still_fails_the_commit_comparison(self):
        target = self.bundle / "RuntimeRequest.ts"
        target.write_text("export type RuntimeRequest = any;\n")
        self.rewrite_manifest()
        self.write_source()  # accept the new manifest hash as the pin
        self.assertDrift("source hash mismatch|divergent")

    def test_extra_file_is_refused(self):
        (self.bundle / "RuntimeExtra.ts").write_text("export type Extra = true;\n")
        self.assertDrift("missing or extra")

    def test_missing_file_is_refused(self):
        (self.bundle / "RuntimeRequest.ts").unlink()
        self.assertDrift("missing or extra")

    def test_stale_commit_pin_is_refused(self):
        self.write_source(core_commit="0" * 40)
        self.assertDrift("not the pinned Core bytes")

    def test_source_without_full_commit_is_refused(self):
        self.write_source(core_commit="HEAD")
        self.assertDrift("immutable core_commit")

    def test_wrong_contract_is_refused(self):
        self.write_source(contract="something.else")
        self.assertDrift("another contract")

    def test_gate_result_is_reported(self):
        ok, _ = verify.run_gate("true", self.repo)
        bad, tail = verify.run_gate("false", self.repo)
        self.assertTrue(ok)
        self.assertFalse(bad)
        self.assertIsInstance(tail, str)


class Drift(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="rt-drift-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "consumer"
        bundle = self.repo / VENDOR / "v1"
        shutil.copytree(CORE_BUNDLE, bundle)
        manifest_bytes = (bundle / "manifest.json").read_bytes()
        manifest = json.loads(manifest_bytes)
        core_root, core_ref = make_core(self.root / "core")
        self.core_root = core_root
        self.core_state = {
            "path": core.BUNDLE.as_posix(),
            "head": core_ref,
            "manifest_sha256": sha256(manifest_bytes),
            "worktree_clean": True,
        }
        (bundle.parent / "SOURCE.json").write_text(
            json.dumps(
                {
                    "contract": "oqto.runtime-interactions",
                    "version": 1,
                    "core_commit": core_ref,
                    "core_path": core.BUNDLE.as_posix(),
                    "manifest_sha256": self.core_state["manifest_sha256"],
                    "file_count": len(manifest["files"]) + 1,
                    "input_count": len(manifest["inputs"]),
                }
            )
        )
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        subprocess.run(["git", "-C", str(self.repo), "add", "-A"], check=True)
        subprocess.run(
            [
                "git",
                "-C",
                str(self.repo),
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "vendor bundle",
            ],
            check=True,
        )
        self.entry = {
            "name": "consumer",
            "repo": "consumer",
            "bundle": (VENDOR / "v1").as_posix(),
            "state": "vendored",
        }

    def test_current_consumer(self):
        row = drift.consumer_state(
            self.entry, self.root, self.core_state, fetch=False, core_root=self.core_root
        )
        self.assertEqual(row["verdict"], "current", row)

    def test_stale_pin_is_reported(self):
        newer = dict(self.core_state, manifest_sha256="f" * 64)
        row = drift.consumer_state(
            self.entry, self.root, newer, fetch=False, core_root=self.core_root
        )
        self.assertEqual(row["verdict"], "stale", row)
        self.assertIn("behind", row)

    def test_divergent_bytes_are_reported(self):
        (self.repo / VENDOR / "v1" / "RuntimeRequest.ts").write_text("export type X = 1;\n")
        row = drift.consumer_state(
            self.entry, self.root, self.core_state, fetch=False, core_root=self.core_root
        )
        self.assertEqual(row["verdict"], "divergent", row)

    def test_missing_checkout_and_planned_rows(self):
        row = drift.consumer_state(
            dict(self.entry, repo="absent"),
            self.root,
            self.core_state,
            fetch=False,
            core_root=self.core_root,
        )
        self.assertEqual(row["verdict"], "missing-checkout")
        planned = drift.consumer_state(
            dict(self.entry, state="planned"),
            self.root,
            self.core_state,
            fetch=False,
            core_root=self.core_root,
        )
        self.assertEqual(planned["verdict"], "not-adopted")


if __name__ == "__main__":
    unittest.main()
