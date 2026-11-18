"""Disposable root-resolution proof; no actual frontend/backend build tools."""

import shlex
import shutil
import subprocess
import tempfile
import unittest
from contextlib import contextmanager
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "dev/agent-check-on-change.sh"


@contextmanager
def agent_check_fixture():
    with tempfile.TemporaryDirectory(prefix="oqto-agent-check-root-") as raw:
        root = Path(raw)
        for directory in ["scripts/dev", "scripts/lint", "backend", "frontend", "bin"]:
            (root / directory).mkdir(parents=True)
        shutil.copyfile(SCRIPT, root / "scripts/dev/check.sh")
        (root / ".gitignore").write_text(".tmp/\ntrace\n")
        subprocess.run(["git", "init", "-q", str(root)], check=True)
        for name in ["cargo", "bun", "just"]:
            tool = root / "bin" / name
            tool.write_text(
                f'#!/bin/sh\nprintf "{name} %s %s\\n" "$PWD" "$*" >> "$TRACE"\n'
            )
            tool.chmod(0o700)
        guard = root / "scripts/lint/rust-ai-guardrails.sh"
        guard.write_text('#!/bin/sh\nprintf "guard %s\\n" "$PWD" >> "$TRACE"\n')
        guard.chmod(0o700)
        env = {
            "PATH": str(root / "bin") + ":/usr/bin:/bin",
            "HOME": str(root),
            "TRACE": str(root / "trace"),
        }
        yield root, env, ["bash", str(root / "scripts/dev/check.sh")]


def isolated_hash_environment(root, env, available):
    """Provide only declared fixture tools, never ambient hash executables."""
    for tool in ["bash", "dirname", "mkdir", "git", "grep", "awk", "readlink", "cat"]:
        executable = shutil.which(tool)
        if executable is None:
            raise RuntimeError(f"Test host lacks {tool}")
        (root / "bin" / tool).symlink_to(executable)
    if available:
        sha = shutil.which("sha256sum")
        native_hash = [sha] if sha else [shutil.which("shasum"), "-a", "256"]
        if native_hash[0] is None:
            raise RuntimeError("Test host lacks a SHA-256 tool")
        hasher = root / "bin/shasum"
        hasher.write_text(
            "#!/usr/bin/env bash\n"
            'printf "fallback\\n" >> "$HASH_TRACE"\n'
            "shift 2\n"
            f'exec {shlex.join(native_hash)} "$@"\n'
        )
        hasher.chmod(0o700)
    return {
        **env,
        "PATH": str(root / "bin"),
        "HASH_TRACE": str(root / ".tmp/hash-used"),
    }


class AgentCheckRoot(unittest.TestCase):
    def run_check(self, command, env):
        result = subprocess.run(
            command, check=False, env=env, cwd="/tmp", capture_output=True, text=True
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result

    def test_quick_full_and_unchanged_profiles_use_repository_root(self):
        with agent_check_fixture() as (root, env, command):
            self.run_check(command, env)
            self.assertEqual(
                (root / "trace").read_text().splitlines(),
                [
                    f"guard {root}",
                    f"cargo {root}/backend fmt --check",
                    f"bun {root}/frontend run lint",
                ],
            )
            self.assertTrue((root / ".tmp/agent-check.last").is_file())
            self.assertIn("skipping", self.run_check(command, env).stdout)
            full = self.run_check(command + ["full"], env)
            self.assertNotIn(
                "skipping", full.stdout, "quick stamp must not certify the full profile"
            )
            (root / "changed").write_text("fixture")
            self.run_check(command + ["full"], env)
            self.assertEqual(
                (root / "trace").read_text().splitlines()[-1], f"just {root} lint"
            )
            self.assertFalse((root / "scripts/.tmp").exists())
            (root / "changed").write_text("new contents")
            self.assertNotIn("skipping", self.run_check(command + ["full"], env).stdout)

    def test_shasum_fallback_and_missing_hasher_fail_closed(self):
        for available in [True, False]:
            with (
                self.subTest(hasher_available=available),
                agent_check_fixture() as (root, env, command),
            ):
                env = isolated_hash_environment(root, env, available)
                if available:
                    self.run_check(command, env)
                    self.assertTrue((root / ".tmp/hash-used").is_file())
                    self.assertIn("skipping", self.run_check(command, env).stdout)
                    for value in ["fixture", "changed bytes"]:
                        (root / "changed").write_text(value)
                        self.assertNotIn(
                            "skipping", self.run_check(command, env).stdout
                        )
                else:
                    result = subprocess.run(
                        command,
                        check=False,
                        env=env,
                        cwd="/tmp",
                        capture_output=True,
                        text=True,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("SHA-256 tool unavailable", result.stderr)
                    self.assertFalse((root / "trace").exists())
                    self.assertFalse((root / ".tmp/agent-check.last").exists())


if __name__ == "__main__":
    unittest.main()
