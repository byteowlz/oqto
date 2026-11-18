"""Synthetic tamper tests for the runtime contract verifier; no runtime access."""

import json
import shutil
import tempfile
import unittest
from pathlib import Path

from runtime_interactions_contract import BUNDLE, ROOT, sha256, verify


class RuntimeContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="runtime-contract-")
        self.addCleanup(self.temp.cleanup)
        self.bundle = Path(self.temp.name) / "v1"
        shutil.copytree(ROOT / BUNDLE, self.bundle)

    def change_manifest(self, change):
        path = self.bundle / "manifest.json"
        manifest = json.loads(path.read_bytes())
        change(manifest)
        path.write_text(json.dumps(manifest))

    def test_complete_bundle_and_schema_corpus(self):
        counts = verify(self.bundle)
        self.assertEqual(counts["request"], 11)
        self.assertEqual(counts["invalid"], 20)

    def test_extra_file_refused(self):
        (self.bundle / "extra.ts").write_text("export type Fake = true;\n")
        with self.assertRaisesRegex(ValueError, "extra"):
            verify(self.bundle)

    def test_missing_file_refused(self):
        (self.bundle / "RuntimeRequest.ts").unlink()
        with self.assertRaisesRegex(ValueError, "missing"):
            verify(self.bundle)

    def test_modified_type_refused(self):
        (self.bundle / "RuntimeRequest.ts").write_text(
            "export type RuntimeRequest = any;\n"
        )
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            verify(self.bundle)

    def test_source_mismatch_refused(self):
        self.change_manifest(lambda m: m["inputs"].__setitem__("LICENSE", "0" * 64))
        with self.assertRaisesRegex(ValueError, "source hash mismatch"):
            verify(self.bundle)

    def test_symlink_refused(self):
        path = self.bundle / "RuntimeRequest.ts"
        path.unlink()
        path.symlink_to(ROOT / BUNDLE / "RuntimeRequest.ts")
        with self.assertRaisesRegex(ValueError, "regular files"):
            verify(self.bundle)

    def assert_permissive_schema_refused(self, name, message):
        schema = self.bundle / name
        schema.write_text("{}\n")
        self.change_manifest(
            lambda m: m["files"].__setitem__(schema.name, sha256(schema.read_bytes()))
        )
        with self.assertRaisesRegex(ValueError, message):
            verify(self.bundle)

    def test_rehashed_permissive_schema_fails_negative_corpus(self):
        self.assert_permissive_schema_refused(
            "request.schema.json", "invalid fixture accepted"
        )

    def test_rehashed_permissive_source_frame_schema_fails_negative_corpus(self):
        self.assert_permissive_schema_refused(
            "source-frame.schema.json", "invalid source fixture accepted"
        )

    def test_mutable_source_ref_refused(self):
        with self.assertRaisesRegex(ValueError, "immutable commit"):
            verify(self.bundle, source_ref="HEAD")


if __name__ == "__main__":
    unittest.main()
