#!/usr/bin/env python3
"""Offline, privileged-action-free release asset publication checks."""

import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/release/check-release-assets.py"
spec = importlib.util.spec_from_file_location("release_assets", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
TAG = "v0.5.0"
TARGET = module.TARGET
PREFIX = f"oqto-{TAG}-{TARGET}"


def bundle(root: Path, *, flat=False, frontend=True, target="full", extra=None,
           executable=True, duplicate=False, symlink=False) -> Path:
    artifact = root / f"{PREFIX}.tar.gz"
    items = {f"{PREFIX}/manifest.toml":
             f'manifest_version = 1\nid = "oqto-dist"\n[release]\ntarget = "{target}"\n'.encode()}
    for name in module.REQUIRED_BINS:
        items[f"{PREFIX}/{'bin' if flat else 'immutable/bin'}/{name}"] = b"#!/bin/true\n"
    if frontend:
        items[f"{PREFIX}/immutable/frontend/dist/index.html"] = b"<!doctype html>"
    if extra:
        items[extra] = b"unsafe"
    with tarfile.open(artifact, "w:gz") as archive:
        for name, content in items.items():
            member = tarfile.TarInfo(name)
            member.size = len(content)
            member.mode = 0o644 if name.endswith("manifest.toml") or name.endswith("index.html") else (0o755 if executable else 0o644)
            archive.addfile(member, io.BytesIO(content))
        if duplicate:
            member = tarfile.TarInfo(f"{PREFIX}/manifest.toml")
            member.size = 0
            archive.addfile(member, io.BytesIO())
        if symlink:
            member = tarfile.TarInfo(f"{PREFIX}/immutable/bin/unmanaged")
            member.type = tarfile.SYMTYPE
            member.linkname = "/etc/passwd"
            archive.addfile(member)
    return artifact


class ReleaseAssetTests(unittest.TestCase):
    def test_canonical_bundle_is_accepted(self):
        with tempfile.TemporaryDirectory() as directory:
            module.check_artifact(bundle(Path(directory)), TAG)

    def test_legacy_flat_and_missing_frontend_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            for options in ({"flat": True}, {"frontend": False}):
                with self.assertRaisesRegex(ValueError, "incomplete full release"):
                    module.check_artifact(bundle(Path(directory), **options), TAG)

    def test_non_full_target_or_non_executable_binary_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            for options in ({"target": "runner_only"}, {"executable": False}):
                with self.assertRaises(ValueError):
                    module.check_artifact(bundle(Path(directory), **options), TAG)

    def test_path_traversal_and_duplicate_paths_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "unsafe release member"):
                module.check_artifact(bundle(Path(directory), extra=f"{PREFIX}/../escape"), TAG)
            with self.assertRaisesRegex(ValueError, "duplicate release member"):
                module.check_artifact(bundle(Path(directory), duplicate=True), TAG)
            with self.assertRaisesRegex(ValueError, "unsafe release member"):
                module.check_artifact(bundle(Path(directory), symlink=True), TAG)


if __name__ == "__main__":
    unittest.main()
