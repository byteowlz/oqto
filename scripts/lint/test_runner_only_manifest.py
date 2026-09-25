"""Offline artifact contract checks; not an installation or runtime doctor proof."""

import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "dist/manifest.toml"
VALIDATOR = ROOT / "scripts/lint/verify-dist-manifest.py"


class RunnerArtifactTests(unittest.TestCase):
    def check_manifest(self, text, target="runner_only"):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "manifest.toml"
            path.write_text(text)
            return subprocess.run(
                ["python3", str(VALIDATOR), "--manifest", str(path), "--target", target,
                 "--allow-missing-binaries", "--allow-missing-extensions"],
                cwd=ROOT, capture_output=True, text=True, check=False,
            )

    def test_runner_accepts_absent_web_assets(self):
        text = MANIFEST.read_text()
        start = text.index('[[assets]]\nid = "bin-oqto"')
        end = text.index('[[assets]]\nid = "bin-oqtoctl"')
        text = text[:start] + text[end:]
        self.assertEqual(self.check_manifest(text).returncode, 0)

    def test_full_manifest_rejects_missing_or_wrong_install_target(self):
        text = MANIFEST.read_text()
        for altered in (text.replace('target = "full"', 'target = "runner_only"'),
                        text.replace('target = "full"\n', '')):
            self.assertNotEqual(self.check_manifest(altered, "full").returncode, 0)

    def test_missing_runner_asset_rejected(self):
        text = MANIFEST.read_text().replace('id = "bin-oqto-runner"', 'id = "removed-runner"')
        self.assertNotEqual(self.check_manifest(text).returncode, 0)

    def test_transport_and_pi_contract_fail_closed(self):
        text = MANIFEST.read_text()
        for altered in (text.replace('user-systemd-socket', 'remote-mtls'),
                        text.replace('external-required', 'disabled'),
                        text.replace('"bin-oqto-files", ', '')):
            self.assertNotEqual(self.check_manifest(altered).returncode, 0)


if __name__ == "__main__":
    unittest.main()
