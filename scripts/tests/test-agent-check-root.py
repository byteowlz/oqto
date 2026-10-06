#!/usr/bin/env python3
"""Disposable root-resolution proof; no actual frontend/backend build tools."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "dev/agent-check-on-change.sh"


class AgentCheckRoot(unittest.TestCase):
    def test_quick_full_and_unchanged_profiles_use_repository_root(self):
        with tempfile.TemporaryDirectory(prefix="oqto-agent-check-root-") as raw:
            root = Path(raw)
            for directory in ["scripts/dev", "scripts/lint", "backend", "frontend", "bin"]:
                (root / directory).mkdir(parents=True)
            shutil.copyfile(SCRIPT, root / "scripts/dev/check.sh")
            (root / ".gitignore").write_text(".tmp/\ntrace\n")
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            for name in ["cargo", "bun", "just"]:
                tool = root / "bin" / name
                tool.write_text(f'#!/bin/sh\nprintf "{name} %s %s\\n" "$PWD" "$*" >> "$TRACE"\n')
                tool.chmod(0o700)
            guard = root / "scripts/lint/rust-ai-guardrails.sh"
            guard.write_text('#!/bin/sh\nprintf "guard %s\\n" "$PWD" >> "$TRACE"\n')
            guard.chmod(0o700)
            env = {"PATH": str(root / "bin") + ":/usr/bin:/bin", "HOME": str(root), "TRACE": str(root / "trace")}
            command = ["bash", str(root / "scripts/dev/check.sh")]
            subprocess.run(command, env=env, cwd="/tmp", check=True, capture_output=True)
            self.assertEqual((root / "trace").read_text().splitlines(), [
                f"guard {root}", f"cargo {root}/backend fmt --check", f"bun {root}/frontend run lint"])
            self.assertTrue((root / ".tmp/agent-check.last").is_file())
            second = subprocess.run(command, env=env, cwd="/tmp", check=True, capture_output=True, text=True)
            self.assertIn("skipping", second.stdout)
            full = subprocess.run(command + ["full"], env=env, cwd="/tmp", check=True, capture_output=True, text=True)
            self.assertNotIn("skipping", full.stdout, "quick stamp must not certify the full profile")
            # Force a new signature without creating production/user state.
            (root / "changed").write_text("fixture")
            subprocess.run(command + ["full"], env=env, cwd="/tmp", check=True, capture_output=True)
            self.assertEqual((root / "trace").read_text().splitlines()[-1], f"just {root} lint")
            self.assertFalse((root / "scripts/.tmp").exists())
            # Same filename/status, different contents must not reuse a stamp.
            (root / "changed").write_text("new contents")
            changed = subprocess.run(command + ["full"], env=env, cwd="/tmp", check=True, capture_output=True, text=True)
            self.assertNotIn("skipping", changed.stdout)


if __name__ == "__main__":
    unittest.main()
