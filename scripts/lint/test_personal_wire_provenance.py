"""Synthetic-only whole-crate provenance regressions; no checkout or runtime needed."""

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from personal_wire_provenance import verify_vendor

REVISION = "a" * 40
SOURCE_CARGO = b"""[package]
name = "oqto-personal-wire"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
description = "Source fixture"
[features]
default = []
tls = ["dep:tls-lib"]
[dependencies]
serde.workspace = true
tls-lib = { workspace = true, optional = true }
[lints]
workspace = true
"""
PORTABLE_CARGO = b"""[package]
name = "oqto-personal-wire"
version = "0.5.11"
edition = "2024"
license = "MIT"
description = "Portable fixture"
[features]
default = []
tls = ["dep:tls-lib"]
[dependencies]
serde = { version = "1.0", features = ["derive"] }
tls-lib = { version = "0.26", default-features = false, optional = true }
[lints]
workspace = true
"""
WORKSPACE = b"""[workspace.package]
version = "0.5.11"
edition = "2024"
license = "MIT"
authors = ["Fixture contributors"]
[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
tls-lib = { version = "0.26", default-features = false }
"""


class ProvenanceFixture(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "vendor"
        self.canonical = {
            "Cargo.toml": SOURCE_CARGO,
            "LICENSE": b"Synthetic MIT fixture\n",
        }
        for name in ("lib", "actions", "tls", "tests"):
            self.canonical[f"src/{name}.rs"] = f"// synthetic {name}\n".encode()
        for relative, content in self.canonical.items():
            target = self.root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(PORTABLE_CARGO if relative == "Cargo.toml" else content)
        self.metadata = {
            "source": REVISION,
            "files": {
                relative: hashlib.sha256(
                    (self.root / relative).read_bytes()
                ).hexdigest()
                for relative in self.canonical
            },
        }
        self.save_metadata()
        (self.root / "PROVENANCE.md").write_text(f"Source: {REVISION}\n")

    def save_metadata(self) -> None:
        (self.root / "SOURCE.json").write_text(json.dumps(self.metadata))

    def problems(self) -> list[str]:
        return verify_vendor(self.root, REVISION, self.canonical, WORKSPACE)

    def change_cargo(self, old: bytes, new: bytes) -> None:
        target = self.root / "Cargo.toml"
        target.write_bytes(target.read_bytes().replace(old, new))
        self.metadata["files"]["Cargo.toml"] = hashlib.sha256(
            target.read_bytes()
        ).hexdigest()
        self.save_metadata()


class IdentityTests(ProvenanceFixture):
    def test_complete_portable_crate_passes(self) -> None:
        self.assertEqual(self.problems(), [])

    def test_stale_recorded_hash_fails(self) -> None:
        self.metadata["files"]["src/actions.rs"] = "b" * 64
        self.save_metadata()
        self.assertIn(
            "src/actions.rs: recorded SHA256 differs from vendored bytes",
            self.problems(),
        )

    def test_rehashing_modified_source_does_not_bless_it(self) -> None:
        content = b"// local addition, still not the selected upstream\n"
        (self.root / "src/actions.rs").write_bytes(content)
        self.metadata["files"]["src/actions.rs"] = hashlib.sha256(content).hexdigest()
        self.save_metadata()
        self.assertIn(
            "src/actions.rs: bytes differ from selected canonical revision",
            self.problems(),
        )

    def test_wrong_source_revision_fails(self) -> None:
        self.metadata["source"] = "b" * 40
        self.save_metadata()
        self.assertIn("SOURCE.json: source revision mismatch", self.problems())

    def test_human_provenance_must_name_selected_full_revision(self) -> None:
        (self.root / "PROVENANCE.md").write_text(
            "An old revision plus a local change\n"
        )
        self.assertIn("PROVENANCE.md: full selected revision absent", self.problems())

    def test_mutable_or_short_revision_fails(self) -> None:
        for ref in ("main", "abcdef0", "A" * 40):
            self.assertTrue(verify_vendor(self.root, ref, self.canonical, WORKSPACE))


class CoverageTests(ProvenanceFixture):
    def test_missing_cargo_hash_fails(self) -> None:
        del self.metadata["files"]["Cargo.toml"]
        self.save_metadata()
        self.assertIn(
            "SOURCE.json: file set differs from complete canonical crate",
            self.problems(),
        )

    def test_future_upstream_module_is_not_silently_excluded(self) -> None:
        self.canonical["src/new_module.rs"] = b"// another normative source\n"
        self.assertTrue(self.problems())

    def test_unrecorded_automatic_build_script_fails(self) -> None:
        (self.root / "build.rs").write_text("// cannot silently add build behavior\n")
        self.assertIn(
            "vendor: file set differs from canonical crate plus provenance",
            self.problems(),
        )

    def test_symlinked_source_fails_without_reading_its_target(self) -> None:
        source = self.root / "src/actions.rs"
        source.unlink()
        source.symlink_to(self.root.parent / "absent-outside-file")
        self.assertIn(
            "src/actions.rs: unsafe/missing content or unsupported normalization",
            self.problems(),
        )

    def test_invalid_metadata_fails_closed(self) -> None:
        (self.root / "SOURCE.json").write_text("[]")
        self.assertTrue(self.problems())


class CargoTests(ProvenanceFixture):
    def test_normalization_cannot_change_dependency_version(self) -> None:
        self.change_cargo(b'version = "0.26"', b'version = "0.27"')
        self.assertIn(
            "Cargo.toml: unsafe/missing content or unsupported normalization",
            self.problems(),
        )

    def test_normalization_cannot_enable_default_crypto_features(self) -> None:
        self.change_cargo(b"default-features = false", b"default-features = true")
        self.assertTrue(self.problems())

    def test_normalization_cannot_drop_tls_feature(self) -> None:
        self.change_cargo(b'tls = ["dep:tls-lib"]', b"tls = []")
        self.assertTrue(self.problems())

    def test_normalization_cannot_change_package_version(self) -> None:
        self.change_cargo(b'version = "0.5.11"', b'version = "0.5.12"')
        self.assertTrue(self.problems())

    def test_exact_source_cargo_without_workspace_resolution_fails(self) -> None:
        (self.root / "Cargo.toml").write_bytes(SOURCE_CARGO)
        self.assertTrue(self.problems())


if __name__ == "__main__":
    unittest.main()
