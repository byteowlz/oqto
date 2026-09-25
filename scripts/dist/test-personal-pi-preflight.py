#!/usr/bin/env python3
"""Offline, noninteractive regression tests for first-install Pi preflight."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("personal-pi-preflight.py")
spec = importlib.util.spec_from_file_location("personal_pi_preflight", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PersonalPiPreflightTests(unittest.TestCase):
    def test_missing_binary_and_auth_are_not_chat_ready(self):
        with tempfile.TemporaryDirectory() as root:
            result = module.inspect(Path(root) / "pi", Path(root) / "agent", "direct", None)
            self.assertFalse(result["binary_acquired"])
            self.assertFalse(result["credential_file_present"])
            self.assertEqual(result["model_selection"], "missing")
            self.assertEqual(result["chat_readiness"], "not_verified")

    def test_existing_config_is_untouched_and_never_exposed(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            pi = root / "pi"
            pi.write_text("#!/bin/sh\nexit 1\n")
            pi.chmod(0o700)
            agent = root / "agent"
            agent.mkdir()
            secret = b'{"secret":"DO_NOT_PRINT"}\n'
            (agent / "auth.json").write_bytes(secret)
            (agent / "settings.json").write_text('{"custom":true}')
            before = {p.name: (p.read_bytes(), p.stat().st_mtime_ns) for p in agent.iterdir()}
            for route in ("direct", "eavs"):
                output = subprocess.check_output([
                    sys.executable, str(SCRIPT), "--pi", str(pi),
                    "--agent-dir", str(agent), "--route", route, "--model", "candidate",
                ], env={**os.environ, "HOME": str(root), "PI_CODING_AGENT_DIR": str(agent)})
                result = json.loads(output)
                self.assertTrue(result["binary_acquired"])
                self.assertTrue(result["credential_file_present"])
                self.assertEqual(result["human_consent"], "not_verified")
                self.assertEqual(result["provider_auth"], "not_verified")
                self.assertEqual(result["model_selection"], "declared_not_verified")
                self.assertEqual(result["chat_readiness"], "not_verified")
                self.assertNotIn(b"DO_NOT_PRINT", output)
                self.assertEqual(route, result["provider_route"])
                self.assertEqual(route == "eavs", "EAVS" in " ".join(result["next_steps"]))
            after = {p.name: (p.read_bytes(), p.stat().st_mtime_ns)
                     for p in agent.iterdir()}
            self.assertEqual(before, after)

    def test_fresh_home_stays_fresh_and_route_is_required(self):
        with tempfile.TemporaryDirectory() as root:
            agent = Path(root) / ".pi" / "agent"
            result = subprocess.run([
                sys.executable, str(SCRIPT), "--pi", str(Path(root) / "pi"),
                "--agent-dir", str(agent), "--route", "direct",
            ], capture_output=True, check=True)
            self.assertFalse(json.loads(result.stdout)["credential_file_present"])
            self.assertFalse(agent.exists())
            missing_choice = subprocess.run([
                sys.executable, str(SCRIPT), "--pi", str(Path(root) / "pi"),
                "--agent-dir", str(agent),
            ], capture_output=True)
            self.assertNotEqual(missing_choice.returncode, 0)
            self.assertFalse(agent.exists())


if __name__ == "__main__":
    unittest.main()
