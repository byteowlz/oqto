#!/usr/bin/env python3
import base64
import contextlib
import copy
import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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

    def source_fixture(self, root):
        contents = [
            '[external]\npi = "1.0.0"\nnode = "24.22.2"\npi-linux-x64-sha256 = "old"\npi-linux-arm64-sha256 = "old"\n',
            '[release]\npi_version = "1.0.0"\npi_npm_integrity = "old"\npi_package_lock_sha256 = "old"\n',
            '{"name":"fixture-runtime","version":"1.0.0"}',
            "{}",
        ]
        for name, content in zip(mod.PIN_PATHS, contents):
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        return {n: (root / n).read_bytes() for n in mod.PIN_PATHS}

    def mocks(self, changes):
        stack = contextlib.ExitStack()
        stack.enter_context(
            mock.patch.object(mod, "source_lock", return_value=contextlib.nullcontext())
        )
        stack.enter_context(mock.patch.object(mod, "clean_source"))
        stack.enter_context(
            mock.patch.object(mod, "candidate_closure", return_value=({}, {}, {}))
        )
        stack.enter_context(mock.patch.object(mod, "render_pins", return_value=changes))
        return stack

    def test_dirty_checkout_refused_without_stash_or_reset(self):
        with (
            mock.patch.object(
                mod.subprocess, "check_output", return_value=" M foreign.rs\n"
            ),
            self.assertRaisesRegex(ValueError, "clean canonical"),
        ):
            mod.clean_source(Path("unused"))

    def test_current_is_noop_and_major_is_visible_failure(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            before = self.source_fixture(root)
            with self.mocks({}):
                result = mod.update_source(root, root / "evidence", metadata("1.0.0"))
                self.assertEqual(result["status"], "current")
                self.assertFalse((root / "evidence").exists())
                with self.assertRaisesRegex(ValueError, "new major"):
                    mod.update_source(root, root / "evidence", metadata("2.0.0"))
            self.assertEqual(
                before, {n: (root / n).read_bytes() for n in mod.PIN_PATHS}
            )

    def test_failed_qualification_restores_only_owned_files(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            before = self.source_fixture(root)
            (root / "foreign.txt").write_text("preserve")
            changes = {n: "candidate" for n in mod.PIN_PATHS}
            with (
                self.mocks(changes),
                mock.patch.object(
                    mod,
                    "qualify",
                    side_effect=subprocess.CalledProcessError(1, "fixture-gate"),
                ),
                self.assertRaises(subprocess.CalledProcessError),
            ):
                mod.update_source(root, root / "evidence", metadata())
            self.assertEqual(
                before, {n: (root / n).read_bytes() for n in mod.PIN_PATHS}
            )
            self.assertEqual((root / "foreign.txt").read_text(), "preserve")

    def test_success_changes_exact_four_files_and_records_scope(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            self.source_fixture(root)
            changes = {n: "candidate" for n in mod.PIN_PATHS}
            with self.mocks(changes), mock.patch.object(mod, "qualify") as gate:
                result = mod.update_source(root, root / "evidence", metadata())
            gate.assert_called_once()
            self.assertEqual(result["status"], "updated")
            self.assertTrue(result["accepted_pin_changed"])
            self.assertIn("not-deployment", result["qualification_scope"])
            self.assertEqual(
                json.loads((root / "evidence/report.json").read_text()), result
            )

    def test_concurrent_edits_are_not_overwritten_during_failure(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            self.source_fixture(root)
            changes = {n: "candidate" for n in mod.PIN_PATHS}

            def edit_during_gate(*_):
                (root / mod.PIN_PATHS[0]).write_text("foreign edit")

            with (
                self.mocks(changes),
                mock.patch.object(mod, "qualify", side_effect=edit_during_gate),
                self.assertRaisesRegex(ValueError, "concurrently"),
            ):
                mod.update_source(root, root / "evidence", metadata())
            self.assertEqual((root / mod.PIN_PATHS[0]).read_text(), "foreign edit")
            self.assertFalse((root / "evidence/report.json").exists())

    def test_verify_refuses_to_rewrite_rebased_or_no_longer_latest_pins(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            before = self.source_fixture(root)
            with self.mocks({n: "changed" for n in mod.PIN_PATHS}):
                with self.assertRaisesRegex(ValueError, "latest stable moved"):
                    mod.update_source(root, root / "evidence", metadata(), verify=True)
                with self.assertRaisesRegex(ValueError, "rebased pins"):
                    mod.update_source(
                        root, root / "evidence", metadata("1.0.0"), verify=True
                    )
            self.assertEqual(
                before, {n: (root / n).read_bytes() for n in mod.PIN_PATHS}
            )

    def test_render_preserves_node_and_syncs_lock_hash(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            self.source_fixture(root)
            plan = mod.discover("1.0.0", metadata())
            changes = mod.render_pins(
                root,
                plan,
                {},
                {},
                {"pi-linux-x64.tar.gz": "a" * 64, "pi-linux-arm64.tar.gz": "b" * 64},
            )
            self.assertEqual(set(changes), set(mod.PIN_PATHS))
            self.assertIn('node = "24.22.2"', changes[mod.PIN_PATHS[0]])
            self.assertIn('pi_version = "1.0.3"', changes[mod.PIN_PATHS[1]])
            plan["node_engine"] = ">=26.0.0"
            with self.assertRaisesRegex(ValueError, "newer Node"):
                mod.render_pins(root, plan, {}, {}, {})

    def test_release_asset_bytes_and_origin_are_verified(self):
        name = "fixture.json"
        url = f"https://github.com/earendil-works/pi/releases/download/v1.0.3/{name}"
        release = {
            "assets": [
                {
                    "name": name,
                    "browser_download_url": url,
                    "digest": "sha256:" + mod.hashlib.sha256(b"safe").hexdigest(),
                }
            ]
        }
        with mock.patch.object(mod, "fetch", return_value=b"safe"):
            self.assertEqual(mod.publisher_asset(release, "1.0.3", name), b"safe")
        with (
            mock.patch.object(mod, "fetch", return_value=b"tampered"),
            self.assertRaisesRegex(ValueError, "digest mismatch"),
        ):
            mod.publisher_asset(release, "1.0.3", name)
        release["assets"][0]["browser_download_url"] = (
            "https://example.invalid/fixture.json"
        )
        with self.assertRaisesRegex(ValueError, "origin mismatch"):
            mod.publisher_asset(release, "1.0.3", name)

    def test_existing_evidence_and_symlinked_pins_are_not_replaced(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw).resolve()
            before = self.source_fixture(root)
            output = root / "evidence"
            output.mkdir()
            (output / "sentinel").write_text("preserve")
            with self.mocks({}), self.assertRaises(FileExistsError):
                mod.update_source(root, output, metadata())
            self.assertEqual(
                before, {n: (root / n).read_bytes() for n in mod.PIN_PATHS}
            )
            self.assertEqual((output / "sentinel").read_text(), "preserve")
            (root / mod.PIN_PATHS[0]).unlink()
            (root / "foreign.toml").write_bytes(before[mod.PIN_PATHS[0]])
            (root / mod.PIN_PATHS[0]).symlink_to(root / "foreign.toml")
            with self.mocks({}), self.assertRaisesRegex(ValueError, "regular file"):
                mod.update_source(root, root / "new-output", metadata())
            self.assertEqual(
                (root / "foreign.toml").read_bytes(), before[mod.PIN_PATHS[0]]
            )

    def test_partial_source_write_failure_is_atomic_and_recoverable(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw).resolve()
            before = self.source_fixture(root)
            changes = {n: "candidate" for n in mod.PIN_PATHS}
            replace = mod.os.replace
            calls = 0

            def fail_once(src, dst):
                nonlocal calls
                calls += 1
                if calls == 2:
                    raise OSError("synthetic acquisition disk failure")
                return replace(src, dst)

            with (
                self.mocks(changes),
                mock.patch.object(mod.os, "replace", fail_once),
                self.assertRaises(OSError),
            ):
                mod.update_source(root, root / "evidence", metadata())
            self.assertEqual(
                before, {n: (root / n).read_bytes() for n in mod.PIN_PATHS}
            )
            self.assertFalse(list(root.rglob(".pi-pin-*")))

    def test_failed_report_write_restores_owned_pin_bytes(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw).resolve()
            before = self.source_fixture(root)
            changes = {n: "candidate" for n in mod.PIN_PATHS}
            write = Path.write_text

            def fail_report(path, data, **kwargs):
                if path.name == "report.json":
                    raise OSError("synthetic full disk")
                return write(path, data, **kwargs)

            with (
                self.mocks(changes),
                mock.patch.object(mod, "qualify"),
                mock.patch.object(Path, "write_text", fail_report),
                self.assertRaises(OSError),
            ):
                mod.update_source(root, root / "evidence", metadata())
            self.assertEqual(
                before, {n: (root / n).read_bytes() for n in mod.PIN_PATHS}
            )

    def test_qualification_does_not_pass_workflow_tokens_or_personal_home(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw).resolve()
            self.source_fixture(root)
            output = root / "evidence"
            output.mkdir()
            with (
                mock.patch.object(
                    mod, "shipped_extensions", return_value=root / "extensions"
                ),
                mock.patch.object(mod.subprocess, "run") as run,
                mock.patch.dict(
                    mod.os.environ,
                    {
                        "FORGEJO_TOKEN": "synthetic",
                        "GITHUB_TOKEN": "synthetic",
                        "PUSH_TOKEN": "synthetic",
                    },
                ),
            ):
                mod.qualify(root, output)
            self.assertEqual(run.call_count, 5)
            for call in run.call_args_list:
                env = call.kwargs["env"]
                self.assertTrue(str(env["HOME"]).startswith(str(output)))
                self.assertFalse(
                    {"FORGEJO_TOKEN", "GITHUB_TOKEN", "PUSH_TOKEN"} & env.keys()
                )

    def test_daily_workflow_is_private_scheduled_and_actions_are_pinned(self):
        import re

        path = ROOT / ".github/workflows/pi-updates.yml"
        if not path.exists():
            self.skipTest("Private Forgejo workflow is intentionally not exported")
        workflow = path.read_text()
        self.assertIn("cron: '23 6 * * *'", workflow)
        self.assertIn("workflow_dispatch:", workflow)
        self.assertIn("github.server_url != 'https://github.com'", workflow)
        self.assertIn("github.ref == 'refs/heads/main'", workflow)
        self.assertIn("persist-credentials: false", workflow)
        self.assertIn("HEAD:refs/heads/main", workflow)
        self.assertNotIn("--force", workflow)
        uses = re.findall(r"uses: (\S+)", workflow)
        self.assertTrue(uses)
        self.assertTrue(
            all(re.fullmatch(r"https://[^@]+@[a-f0-9]{40}", value) for value in uses)
        )

    def test_hydration_only_admits_exact_official_workspace_identity(self):
        name = "@earendil-works/pi-ai"
        entry = {"version": "1.0.3"}
        lock = {"packages": {"node_modules/" + name: entry}}
        data = {
            "name": name,
            "version": "1.0.3",
            "dist": {
                "integrity": INTEGRITY,
                "tarball": f"https://registry.npmjs.org/{name}/-/pi-ai-1.0.3.tgz",
            },
        }
        with mock.patch.object(mod, "fetch", return_value=json.dumps(data).encode()):
            mod.hydrate_workspaces(lock)
        self.assertEqual(entry["integrity"], INTEGRITY)
        with self.assertRaisesRegex(ValueError, "non-workspace"):
            mod.hydrate_workspaces(
                {"packages": {"node_modules/unknown": {"version": "1.0.3"}}}
            )


if __name__ == "__main__":
    unittest.main()
