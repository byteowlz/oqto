"""Real CLI no-launch contract. Build oqto --bin oqto-sandbox first.
Run: python3 backend/crates/oqto-sandbox/tests/effective_cli_test.py
Override binary using OQTO_SANDBOX_TEST_BIN.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

BINARY = Path(os.environ.get("OQTO_SANDBOX_TEST_BIN", str(
    Path(__file__).resolve().parents[3] / "target/debug/oqto-sandbox")))


class EffectiveCLI(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.workspace = self.root / "staging"
        self.workspace.mkdir()
        self.config = self.root / "private.toml"
        self.config.write_text('enabled = true\nprofile = "minimal"\ndeny_read = ["~/.pi/agent"]\n')
        self.config.chmod(0o600)

    def invoke(self, *args):
        return subprocess.run([str(BINARY), "--config", str(self.config),
                               "--workspace", str(self.workspace), *args],
                              capture_output=True, text=True, timeout=20)

    def test_json_and_no_payload_execution(self):
        marker = self.workspace / "payload-ran"
        result = self.invoke("--print-effective", "--", "touch", str(marker))
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["schema"], "oqto.sandbox.effective.v1")
        self.assertIn(str(self.workspace), report["allow_write"])
        self.assertIn(str((Path.home() / ".pi/agent").resolve()), report["deny_read"])
        self.assertFalse(report["enforcement_verified"])
        self.assertFalse(marker.exists())

    def test_check_needs_no_payload_and_stdout_is_empty(self):
        result = self.invoke("--check-config")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_bad_workspace_config_fails_closed(self):
        (self.workspace / ".oqto").mkdir()
        (self.workspace / ".oqto/sandbox.toml").write_text("not valid TOML")
        result = self.invoke("--print-effective")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")

    def test_overlay_preserves_independently_loaded_system_policy(self):
        overlay = self.root / "overlay.toml"
        overlay.write_text('allow_write = []\nhome_access = "none"\n[network]\nmode = "isolated"\n')
        overlay.chmod(0o600)
        self.config.write_text('enabled = true\nprofile = "minimal"\ndeny_read = ["~/.pi/agent"]\n')
        # A private mount namespace supplies a disposable admin config at the real
        # lookup path. No host/user config is changed, and --config is not passed.
        prefix = ["bwrap", "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--bind", str(self.root), str(self.root),
                  "--ro-bind", str(self.config), "/etc/oqto/sandbox.toml", "--", str(BINARY),
                  "--overlay", str(overlay), "--workspace", str(self.workspace)]
        result = subprocess.run([*prefix, "--print-effective", "--json"], capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["network_mode"], "isolated")
        self.assertIn(str((Path.home() / ".pi/agent").resolve()), report["deny_read"])
        self.assertEqual(report["resolved_config"]["allow_write"], [])
        for attempted in ["--no-sandbox", "--config", "--profile"]:
            args = [attempted]
            if attempted != "--no-sandbox":
                args.append(str(self.config) if attempted == "--config" else "minimal")
            rejected = subprocess.run([*prefix, "--check-config", *args], capture_output=True, text=True, timeout=20)
            self.assertNotEqual(rejected.returncode, 0)
        overlay.write_text('extra_rw_bind = ["/"]\n')
        rejected = subprocess.run([*prefix, "--check-config"], capture_output=True, text=True, timeout=20)
        self.assertNotEqual(rejected.returncode, 0)

    def test_unknown_profile_and_disabled_config_fail(self):
        for content in ['profile = "typo-profile"\n', 'profile = "minimal"\nenabled = false\n']:
            with self.subTest(content=content):
                self.config.write_text(content)
                result = self.invoke("--print-effective")
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
