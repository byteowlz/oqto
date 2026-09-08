"""Exercise generated systemd ExecCondition after systemd's dollar unescaping."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class HealthcheckStartupTests(unittest.TestCase):
    def test_bounded_grace_and_unknown_state_fallback(self):
        setup = Path(__file__).resolve().parents[1] / "18-services.sh"
        for scope in ("", "--user"):
            result = subprocess.run(["bash", "-c", 'source "$1"; healthcheck_startup_condition "$2"', "test", str(setup), scope], check=True, capture_output=True, text=True)
            prefix = "ExecCondition=/usr/bin/env bash -c '"
            self.assertTrue(result.stdout.startswith(prefix))
            command = result.stdout.strip()[len(prefix):-1].replace("$$", "$")
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                systemctl = root / "systemctl"
                systemctl.write_text('#!/bin/bash\ncase "$*" in *ActiveState*) echo "$TEST_STATE";; *) echo "$TEST_STARTED";; esac\n')
                systemctl.chmod(0o755)
                cut = root / "cut"
                cut.write_text('#!/bin/bash\necho "$TEST_NOW"\n')
                cut.chmod(0o755)
                cases = [
                    ("active", "900000000", "1000", 1),
                    ("active", "900000000", "1020", 0),
                    ("active", "800000000", "1000", 0),
                    ("inactive", "900000000", "1000", 0),
                    ("failed", "900000000", "1000", 0),
                    ("", "900000000", "1000", 0),
                    ("active", "0", "1000", 0),
                    ("active", "invalid", "1000", 0),
                    ("active", "900000000", "invalid", 0),
                    ("active", "1200000000", "1000", 0),
                ]
                for state, started, now, expected in cases:
                    with self.subTest(scope=scope, state=state, started=started, now=now):
                        outcome = subprocess.run(["/bin/bash", "-c", command], env={**os.environ, "PATH": directory, "TEST_STATE": state, "TEST_STARTED": started, "TEST_NOW": now}, capture_output=True, text=True)
                        self.assertEqual(outcome.returncode, expected, outcome.stderr)
                        self.assertEqual(outcome.stderr, "")


if __name__ == "__main__":
    unittest.main()
