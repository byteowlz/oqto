#!/usr/bin/env python3
import base64
import copy
import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "pi_update", ROOT / "scripts/release/pi-update.py"
)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
INTEGRITY = "sha512-" + base64.b64encode(bytes(64)).decode()


def metadata(version="1.0.3"):
    sdk = {
        "name": mod.PACKAGE,
        "version": version,
        "dist": {
            "integrity": INTEGRITY,
            "tarball": f"https://registry.npmjs.org/{mod.PACKAGE}/-/pi-coding-agent-{version}.tgz",
        },
        "engines": {"node": ">=22.19.0"},
    }
    return {"dist-tags": {"latest": version}, "versions": {version: sdk}}


class PiUpdate(unittest.TestCase):
    def test_candidate_is_exact_unqualified_and_does_not_advance_pin(self):
        plan = mod.discover("1.0.0", metadata())
        self.assertEqual(plan["candidate"], "1.0.3")
        self.assertEqual(plan["status"], "candidate")
        self.assertFalse(plan["qualified"])
        self.assertFalse(plan["accepted_pin_changed"])

    def test_current_and_major_review_are_distinct(self):
        self.assertEqual(mod.discover("1.0.3", metadata())["status"], "current")
        self.assertTrue(
            mod.discover("1.0.3", metadata("2.0.0"))["major_review_required"]
        )

    def test_prerelease_range_malformed_and_downgrade_fail_closed(self):
        for version in [
            "1.0.3-beta.1",
            "latest",
            "^1.0.3",
            "01.0.3",
            "1.2",
            "../1.0.3",
            None,
        ]:
            with self.subTest(version=version), self.assertRaises(ValueError):
                mod.version_key(version)
        with self.assertRaises(ValueError):
            mod.discover("1.0.4", metadata())

    def test_unrelated_historical_version_cannot_override_stable_channel(self):
        data = metadata()
        data["versions"]["99.0.0-beta"] = {}
        self.assertEqual(mod.discover("1.0.0", data)["candidate"], "1.0.3")

    def test_missing_package_bad_name_version_digest_and_origin_are_rejected(self):
        for mutation in ["missing", "name", "version", "digest", "origin"]:
            data = metadata()
            sdk = data["versions"]["1.0.3"]
            if mutation == "missing":
                data["versions"] = {}
            if mutation == "name":
                sdk["name"] = "fake"
            if mutation == "version":
                sdk["version"] = "1.0.2"
            if mutation == "digest":
                sdk["dist"]["integrity"] = "sha512-AAAA"
            if mutation == "origin":
                sdk["dist"]["tarball"] = "https://example.invalid/pkg.tgz"
            with (
                self.subTest(mutation=mutation),
                self.assertRaises((ValueError, TypeError)),
            ):
                mod.discover("1.0.0", data)

    def test_manifest_updates_only_unique_external_exact_pins(self):
        text = '[oqto]\npi = "unchanged"\n[external]\npi = "1.0.0" # baseline\npi-linux-x64-sha256 = "old"\n[other]\npi = "unchanged"\n'
        updated = mod.update_manifest(
            text, {"pi": "1.0.3", "pi-linux-x64-sha256": "new"}
        )
        self.assertEqual(updated.count('pi = "unchanged"'), 2)
        self.assertIn('pi = "1.0.3" # baseline', updated)
        with self.assertRaises(ValueError):
            mod.update_manifest(text, {"missing": "1"})
        with self.assertRaises(ValueError):
            mod.update_manifest(
                text.replace("# baseline", '\npi="duplicate"'), {"pi": "1.0.3"}
            )

    def test_publisher_release_checksum_identity(self):
        names = ["SHA256SUMS", "pi-linux-x64.tar.gz", "pi-linux-arm64.tar.gz"]
        release = {
            "tag_name": "v1.0.3",
            "assets": [
                {
                    "name": n,
                    "browser_download_url": f"https://github.com/earendil-works/pi/releases/download/v1.0.3/{n}",
                }
                for n in names
            ],
        }
        sums = "\n".join("a" * 64 + "  " + n for n in names[1:])
        self.assertEqual(len(mod.release_sums(release, "1.0.3", sums)), 2)
        for bad in [
            {**release, "draft": True},
            {**release, "prerelease": True},
            {**release, "tag_name": "v1.0.2"},
        ]:
            with self.assertRaises(ValueError):
                mod.release_sums(bad, "1.0.3", sums)
        with self.assertRaises(ValueError):
            mod.release_sums(release, "1.0.3", sums + "\n" + sums)

    def test_package_lock_must_match_exact_pins_and_all_closure_integrities(self):
        manifest = {"external": {"pi": "1.0.3"}}
        package = {"dependencies": {mod.PACKAGE: "1.0.3"}}
        lock = {
            "packages": {
                "": {"dependencies": package["dependencies"]},
                "node_modules/" + mod.PACKAGE: {
                    "version": "1.0.3",
                    "integrity": INTEGRITY,
                    "resolved": metadata()["versions"]["1.0.3"]["dist"]["tarball"],
                },
            }
        }
        mod.validate_lock(manifest, package, lock, INTEGRITY)
        for mutation in ["version", "integrity", "origin"]:
            bad = copy.deepcopy(lock)
            sdk = bad["packages"]["node_modules/" + mod.PACKAGE]
            sdk[
                {"version": "version", "integrity": "integrity", "origin": "resolved"}[
                    mutation
                ]
            ] = {
                "version": "1.0.0",
                "integrity": "sha256-x",
                "origin": "http://evil.invalid/x",
            }[mutation]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                mod.validate_lock(manifest, package, bad, INTEGRITY)

    def test_existing_and_symlink_candidate_destinations_are_untouched(self):
        plan = mod.discover("1.0.0", metadata())
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            existing = root / "existing"
            existing.mkdir()
            marker = existing / "marker"
            marker.write_text("keep")
            link = root / "link"
            link.symlink_to(existing)
            for output in [existing, link]:
                with self.assertRaises(ValueError):
                    mod.prepare(root, output, plan, root / "node", {}, {})
            self.assertEqual(marker.read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
