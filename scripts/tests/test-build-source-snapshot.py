#!/usr/bin/env python3
"""Offline proof that a deploy build never reads a moving caller checkout."""

from __future__ import annotations

import json
import os
import signal
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

HELPER = Path(__file__).resolve().parents[1] / "dist/build-source-snapshot.py"


class SnapshotTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory(prefix="oqto snapshot test ")
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.source = self.base / "source checkout"
        self.source.mkdir()
        self.git("init", "-q")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "user.name", "fixture")
        (self.source / "scripts").mkdir()
        (self.source / "input.txt").write_text("commit A\n")
        (self.source / ".gitignore").write_text(
            "frontend/node_modules/\nbackend/target/\n"
        )
        self.script = self.source / "scripts/deploy.sh"
        self.script.write_text(
            "#!/usr/bin/env bash\nset -eu\nprintf 'fixture build\\n'\n"
        )
        self.commit()
        self.original = self.git("rev-parse", "HEAD").strip()
        self.config = self.base / "hosts private.toml"
        self.config.write_text("fixture-host=A\n")
        self.report = self.base / "report.json"
        self.ready = self.base / "ready"
        self.release = self.base / "release"
        self.env = dict(os.environ, XDG_STATE_HOME=str(self.base / "state"))
        self.env.update(
            REPORT=str(self.report), READY=str(self.ready), RELEASE=str(self.release)
        )

    def git(self, *args: str) -> str:
        return subprocess.run(
            ["git", "-C", str(self.source), *args],
            check=True,
            capture_output=True,
            text=True,
        ).stdout

    def commit(self) -> None:
        self.git("add", "-A")
        self.git("commit", "-qm", "fixture")

    def run_helper(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            self.command(*args),
            env=self.env,
            capture_output=True,
            text=True,
            check=False,
        )

    def command(self, *args: str) -> list[str]:
        return [
            "python3",
            str(HELPER),
            "--source",
            str(self.source),
            "--config",
            str(self.config),
            "--",
            *args,
        ]

    def install_probe(self, *, exit_code: int = 0, wait: bool = True) -> None:
        self.script.write_text(
            "#!/usr/bin/env bash\nset -eu\n"
            "exec python3 - \"$@\" <<'PY'\n"
            "import json, os, pathlib, signal, subprocess, sys, time\n"
            "root = pathlib.Path.cwd()\n"
            "args = sys.argv[1:]\n"
            "config_index = len(args) - 1 - args[::-1].index('--config')\n"
            "config = pathlib.Path(args[config_index + 1])\n"
            "def stop(sig, frame):\n    sys.exit(128 + sig)\n"
            "signal.signal(signal.SIGTERM, stop)\n"
            "pathlib.Path(os.environ['READY']).touch()\n"
            + (
                "while not pathlib.Path(os.environ['RELEASE']).exists(): time.sleep(0.01)\n"
                if wait
                else ""
            )
            + "data = dict(root=str(root), commit=subprocess.check_output(['git','rev-parse','HEAD'], text=True).strip(), "
            "input=(root/'input.txt').read_text(), config=config.read_text(), "
            "snapshot=os.environ['OQTO_DEPLOY_SNAPSHOT_COMMIT'], "
            "target=os.environ['CARGO_TARGET_DIR'], dependencies=os.environ['OQTO_DIST_CACHE_DIR'], "
            "artifacts=os.environ['OQTO_DEPLOY_ARTIFACT_DIR'], "
            "poison=(root/'frontend/node_modules/poison').exists(), args=args)\n"
            "pathlib.Path(os.environ['REPORT']).write_text(json.dumps(data))\n"
            "pathlib.Path(os.environ['OQTO_DEPLOY_ARTIFACT_DIR'], 'fixture-artifact').write_text('retained')\n"
            f"sys.exit({exit_code})\nPY\n"
        )
        self.commit()
        self.original = self.git("rev-parse", "HEAD").strip()

    def wait_ready(self, process: subprocess.Popen[str]) -> None:
        for _ in range(1000):
            if self.ready.exists():
                return
            if process.poll() is not None:
                self.fail(f"helper exited before build: {process.communicate()}")
            time.sleep(0.01)
        process.terminate()
        process.communicate(timeout=10)
        self.fail("build did not reach the fixture barrier")

    def start(self) -> subprocess.Popen[str]:
        process = subprocess.Popen(
            self.command("--dev"),
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.addCleanup(lambda: process.poll() is None and process.kill())
        return process

    def test_caller_commit_and_branch_switch_cannot_change_build_inputs(self) -> None:
        self.install_probe()
        poison = self.source / "frontend/node_modules/poison"
        poison.parent.mkdir(parents=True)
        poison.write_text("must not be copied")
        self.env["CARGO_TARGET_DIR"] = str(self.source / "backend/target")
        self.env["OQTO_DIST_CACHE_DIR"] = str(self.base / "shared mutable dependencies")
        process = self.start()
        self.wait_ready(process)
        (self.source / "input.txt").write_text("commit B\n")
        self.commit()
        self.git("checkout", "-qb", "fixture-other")
        self.config.write_text("fixture-host=B\n")
        self.release.touch()
        out, error = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 0, out + error)
        result = json.loads(self.report.read_text())
        self.assertEqual(result["commit"], self.original)
        self.assertEqual(result["snapshot"], self.original)
        self.assertEqual(result["input"], "commit A\n")
        self.assertEqual(result["config"], "fixture-host=A\n")
        self.assertFalse(result["poison"])
        self.assertNotEqual(result["target"], self.env["CARGO_TARGET_DIR"])
        self.assertNotEqual(result["dependencies"], self.env["OQTO_DIST_CACHE_DIR"])
        self.assertIn("--no-extension-update", result["args"])
        self.assertFalse(Path(result["root"]).exists())
        self.assertFalse(Path(result["target"]).exists())
        self.assertTrue((Path(result["artifacts"]) / "fixture-artifact").is_file())
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual((self.source / "input.txt").read_text(), "commit B\n")

    def test_dirty_dev_source_is_refused_without_silently_dropping_edits(self) -> None:
        (self.source / "input.txt").write_text("uncommitted\n")
        result = self.run_helper("--dev")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("uncommitted or untracked", result.stderr)
        self.assertFalse(self.ready.exists())

    def test_untracked_source_is_refused(self) -> None:
        (self.source / "untracked.txt").write_text("uncommitted\n")
        result = self.run_helper("--dev")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("uncommitted or untracked", result.stderr)

    def test_build_failure_retains_artifact_and_cleans_owned_scratch(self) -> None:
        self.install_probe(exit_code=7, wait=False)
        result = self.run_helper("--dev")
        self.assertEqual(result.returncode, 7, result.stdout + result.stderr)
        report = json.loads(self.report.read_text())
        self.assertFalse(Path(report["root"]).exists())
        self.assertTrue((Path(report["artifacts"]) / "fixture-artifact").exists())
        self.assertTrue((Path(report["artifacts"]).parent / "build.log").exists())

    def test_relative_host_config_is_resolved_before_changing_directory(self) -> None:
        self.install_probe(wait=False)
        command = self.command("--dev", "--config", "must-not-use-live-config")
        command[command.index("--config") + 1] = self.config.name
        result = subprocess.run(
            command,
            cwd=self.base,
            env=self.env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            json.loads(self.report.read_text())["config"], "fixture-host=A\n"
        )

    def test_driver_refuses_a_foreign_snapshot_marker_before_authentication(
        self,
    ) -> None:
        hosts = self.base / "driver-hosts.toml"
        hosts.write_text(
            '[[host]]\nname = "fixture"\nssh = ""\nlocal = true\n'
            'mode = "single-user"\nuser = "fixture"\n'
        )
        env = dict(
            self.env,
            OQTO_DEPLOY_SNAPSHOT_ROOT=str(self.source),
            OQTO_DEPLOY_SNAPSHOT_COMMIT=self.original,
        )
        driver = Path(__file__).resolve().parents[1] / "deploy.sh"
        result = subprocess.run(
            ["bash", str(driver), "--dev", "--host", "fixture", "--config", str(hosts)],
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("frozen deployment source identity", result.stderr)
        self.assertNotIn("authorizing deployment", result.stdout)

    def test_sigterm_is_forwarded_before_snapshot_cleanup(self) -> None:
        self.install_probe()
        process = self.start()
        self.wait_ready(process)
        process.send_signal(signal.SIGTERM)
        out, error = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 143, out + error)
        runs = list((self.base / "state/oqto/deploy-builds").iterdir())
        self.assertEqual(len(runs), 1)
        self.assertFalse((runs[0] / "source").exists())


if __name__ == "__main__":
    unittest.main()
