#!/usr/bin/env python3
"""Offline adversarial fixtures for the single-binary release bootstrap."""

from __future__ import annotations

import gzip
import hashlib
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "dist/verified-setup-bootstrap.py"
SPEC = importlib.util.spec_from_file_location("verified_setup_bootstrap", SCRIPT)
assert SPEC and SPEC.loader
bootstrap_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bootstrap_module)

ROOT = "oqto-v0.5.0-x86_64-unknown-linux-gnu"
BINS = ("oqto", "oqtoctl", "oqto-setup", "oqto-runner", "oqto-files",
        "oqto-sandbox", "oqto-usermgr", "pi-bridge")


def add_file(archive: tarfile.TarFile, name: str, content: bytes, mode: int = 0o644) -> None:
    info = tarfile.TarInfo(name)
    info.size = len(content)
    info.mode = mode
    archive.addfile(info, io.BytesIO(content))


def make_bundle(folder: Path, variant: str = "valid") -> Path:
    artifact = folder / f"{ROOT}.tar.gz"
    if variant == "oversized":
        header = tarfile.TarInfo(f"{ROOT}/too-large")
        header.size = 2 * 1024 * 1024 * 1024 + 1
        with gzip.open(artifact, "wb") as output:
            output.write(header.tobuf() + bytes(1024))
        return artifact
    with tarfile.open(artifact, "w:gz") as archive:
        add_file(archive, f"{ROOT}/manifest.toml", b'manifest_version = 1\nid = "oqto-dist"\n[release]\ntarget = "full"\n')
        for name in BINS:
            add_file(archive, f"{ROOT}/immutable/bin/{name}", f"#!/bin/sh\necho {name}\n".encode(), 0o755)
        add_file(archive, f"{ROOT}/immutable/frontend/dist/index.html", b"<main>Oqto</main>")
        if variant == "duplicate":
            add_file(archive, f"{ROOT}/immutable/bin/oqto-setup", b"#!/bin/sh\necho replacement\n", 0o755)
        if variant == "traversal":
            add_file(archive, f"{ROOT}/../../outside", b"escape")
        if variant == "second-root":
            add_file(archive, "other-release/payload", b"escape")
        if variant == "sticky-mode":
            add_file(archive, f"{ROOT}/sticky", b"file", 0o1755)
        if variant == "hidden-bin":
            add_file(archive, f"{ROOT}/immutable/bin/.gitkeep", b"placeholder")
        if variant in {"symlink", "hardlink"}:
            item = tarfile.TarInfo(f"{ROOT}/escape")
            item.type = tarfile.SYMTYPE if variant == "symlink" else tarfile.LNKTYPE
            item.linkname = "../../outside"
            archive.addfile(item)
    if variant == "truncated-gzip":
        artifact.write_bytes(artifact.read_bytes()[:-8])
    return artifact


class VerifiedBootstrapTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.private = self.root / "private"
        self.private.mkdir(mode=0o700)
        self.output = self.private / "oqto-setup"

    def checksum(self, artifact: Path, *, named: str | None = None) -> Path:
        digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
        path = self.root / "bundle.sha256"
        path.write_text(f"{digest}  {named or artifact.name}\n")
        return path

    def test_full_bundle_extracts_exact_staged_installer(self) -> None:
        artifact = make_bundle(self.root)
        bootstrap_module.bootstrap(artifact, self.checksum(artifact), self.output)
        self.assertEqual(self.output.read_bytes(), b"#!/bin/sh\necho oqto-setup\n")
        self.assertEqual(self.output.stat().st_mode & 0o777, 0o500)

    def test_corrupt_layouts_never_produce_an_installer(self) -> None:
        for variant in ("duplicate", "traversal", "second-root", "symlink", "hardlink",
                        "sticky-mode", "hidden-bin", "oversized", "truncated-gzip"):
            with self.subTest(variant=variant):
                artifact = make_bundle(self.root, variant)
                with self.assertRaises(bootstrap_module.InvalidBundle):
                    bootstrap_module.bootstrap(artifact, self.checksum(artifact), self.output)
                self.assertFalse(self.output.exists())

    def test_wrong_or_ambiguous_checksum_never_produces_an_installer(self) -> None:
        artifact = make_bundle(self.root)
        checksum = self.checksum(artifact, named="other.tar.gz")
        with self.assertRaises(bootstrap_module.InvalidBundle):
            bootstrap_module.bootstrap(artifact, checksum, self.output)
        checksum = self.checksum(artifact)
        checksum.write_text(checksum.read_text() * 2)
        with self.assertRaises(bootstrap_module.InvalidBundle):
            bootstrap_module.bootstrap(artifact, checksum, self.output)
        checksum.write_text("0" * 64 + f"  {artifact.name}\n")
        with self.assertRaises(bootstrap_module.InvalidBundle):
            bootstrap_module.bootstrap(artifact, checksum, self.output)
        self.assertFalse(self.output.exists())

    def test_output_must_be_private_and_must_not_replace_user_file(self) -> None:
        artifact = make_bundle(self.root)
        checksum = self.checksum(artifact)
        existing = self.private / "oqto-setup"
        existing.write_text("owned")
        with self.assertRaises(bootstrap_module.InvalidBundle):
            bootstrap_module.bootstrap(artifact, checksum, existing)
        self.assertEqual(existing.read_text(), "owned")
        public = self.root / "public"
        public.mkdir(mode=0o755)
        with self.assertRaises(bootstrap_module.InvalidBundle):
            bootstrap_module.bootstrap(artifact, checksum, public / "unsafe-output")
        self.assertFalse((public / "unsafe-output").exists())

    def test_quoted_artifact_path_is_data_not_shell_syntax(self) -> None:
        directory = self.root / "quote'$(touch never-run)"
        directory.mkdir()
        artifact = make_bundle(directory)
        bootstrap_module.bootstrap(artifact, self.checksum(artifact), self.output)
        self.assertFalse((self.root / "never-run").exists())
        self.assertTrue(self.output.is_file())


if __name__ == "__main__":
    unittest.main()
