"""Source entrypoint relocation, without Pi, native builds or real owner state."""

import os
import platform
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import tui_test

ROOT = Path(__file__).resolve().parents[2]
LAUNCHER = ROOT / "tools/oqt/dev.sh"


class SourceLauncherTests(unittest.TestCase):
    def run_source(self, *args):
        with tempfile.TemporaryDirectory(prefix="oqt-source-entry-") as directory:
            work = Path(directory)
            return subprocess.run(
                ["bash", str(LAUNCHER), *args],
                cwd=work,
                env={
                    **os.environ,
                    "HOME": str(work / "absent-home"),
                    "OQT_OWNER_AGENT_DIR": str(work / "absent-owner"),
                    "OQT_PI_SOURCE": str(work / "absent-sdk"),
                },
                text=True,
                check=False,
                capture_output=True,
                timeout=8,
            )

    def test_help_from_an_unrelated_work_directory(self):
        result = self.run_source("--help")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Usage:", result.stdout)

    def test_unknown_options_fail_before_runtime_preparation(self):
        result = self.run_source("--not-a-supported-option")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("Unsupported oqt prototype option", result.stderr)

    @unittest.skipUnless(
        (platform.system(), platform.machine()) in [("Linux", "x86_64"), ("Darwin", "arm64")],
        "source runtime supports Linux x86_64 and macOS arm64 only",
    )
    def test_source_tree_and_children_remain_forbidden_work_directories(self):
        for work in [ROOT, ROOT / "tools/oqt"]:
            with self.subTest(work=work):
                result = self.run_source("--workdir", str(work))
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn("inside oqt's own source tree", result.stderr)

    def test_pty_harness_distinguishes_source_from_installed_entrypoints(self):
        for launcher in [None, Path("/synthetic/installed/oqt")]:
            with (
                self.subTest(launcher=launcher),
                mock.patch.object(tui_test.subprocess, "Popen") as spawn,
            ):
                terminal = tui_test.Terminal(launcher=launcher)
                try:
                    expected = LAUNCHER if launcher is None else launcher
                    self.assertEqual(spawn.call_args.args[0][0], str(expected))
                finally:
                    os.close(terminal.master)
                    terminal.fixture_policy.cleanup()


if __name__ == "__main__":
    unittest.main()
