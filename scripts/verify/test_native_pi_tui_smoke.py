"""Safety regressions for the disposable launcher; no Pi or provider is started."""

import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "native_smoke", Path(__file__).with_name("native-pi-tui-smoke.py")
)
SMOKE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SMOKE)


class IsolatedPreparation(unittest.TestCase):
    def test_provider_credentials_and_operator_profile_do_not_cross_into_pi(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with mock.patch.dict(
                os.environ,
                {
                    "OPENAI_API_KEY": "operator-secret-must-not-be-inherited",
                    "PI_CODING_AGENT_DIR": "/operator-profile-must-not-be-read",
                    "PI_TUI_RPC_LEASE_CONFIRM": "0",
                },
            ):
                env, args = SMOKE.prepare(root, root / "explicit-extension.ts", 9)
            self.assertNotIn("OPENAI_API_KEY", env)
            self.assertEqual(env["PI_CODING_AGENT_DIR"], str(root / "profile"))
            self.assertEqual(env["PI_TUI_RPC_LEASE_CONFIRM"], "1")
            self.assertEqual(env["HOME"], str(root / "home"))
            for flag in [
                "--offline",
                "--no-tools",
                "--no-mcp",
                "--no-extensions",
                "--no-context-files",
                "--no-skills",
                "--no-approve",
            ]:
                self.assertIn(flag, args)
            models = json.loads((root / "profile/models.json").read_text())
            self.assertEqual(list(models["providers"]), ["fixture"])
            self.assertEqual(
                models["providers"]["fixture"]["baseUrl"], "http://127.0.0.1:9/v1"
            )

    def test_synthetic_seed_is_create_only_never_rewrites_a_pi_owned_file(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            SMOKE.prepare(root, root / "explicit-extension.ts", 9)
            source = root / "sessions/disposable.jsonl"
            before = source.read_bytes()
            with self.assertRaises(FileExistsError):
                SMOKE.seed_session(root)
            self.assertEqual(source.read_bytes(), before)
            rows = [json.loads(line) for line in before.splitlines()]
            self.assertEqual(rows[0]["type"], "session")
            self.assertEqual(rows[0]["cwd"], str(root / "work"))
            self.assertEqual(rows[2]["parentId"], rows[1]["id"])


if __name__ == "__main__":
    unittest.main()
