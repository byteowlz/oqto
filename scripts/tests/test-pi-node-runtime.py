#!/usr/bin/env python3
"""Source lock and fail-closed staging contracts; no package downloads."""
import base64
import json
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
STAGE = ROOT / 'scripts/dist/stage-pi-node-runtime.sh'


class RuntimeTests(unittest.TestCase):
    def test_every_registry_dependency_has_strong_integrity(self):
        lock = json.loads((ROOT / 'scripts/dist/pi-node/package-lock.json').read_text())
        manifest = json.loads((ROOT / 'scripts/dist/pi-node/package.json').read_text())
        self.assertEqual(manifest['dependencies'], {'@earendil-works/pi-coding-agent': '1.0.0'})
        self.assertEqual(lock['packages']['']['dependencies'], manifest['dependencies'])
        self.assertEqual(lock['packages']['node_modules/@earendil-works/pi-coding-agent']['version'], '1.0.0')
        for name, entry in lock['packages'].items():
            if name:
                with self.subTest(package=name):
                    self.assertTrue(entry['resolved'].startswith('https://registry.npmjs.org/'))
                    algorithm, encoded = entry['integrity'].split('-', 1)
                    self.assertEqual(algorithm, 'sha512')
                    self.assertEqual(len(base64.b64decode(encoded, validate=True)), 64)

    def test_existing_output_refused_before_acquisition(self):
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            sentinel = root / 'sentinel'
            sentinel.write_text('preserve me')
            result = subprocess.run([str(STAGE), temp], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Refusing to replace', result.stderr)
            self.assertEqual(sentinel.read_text(), 'preserve me')

    def test_bad_archive_never_installs_or_leaves_stage(self):
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            archive = root / 'bad.tar.xz'
            archive.write_bytes(b'not the official Node archive')
            import os
            env = dict(os.environ, PI_NODE_SOURCE_ARCHIVE=str(archive))
            result = subprocess.run([str(STAGE), str(root / 'output')], env=env, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / 'output').exists())
            self.assertFalse(list(root.glob('.pi-node-build.*')))


if __name__ == '__main__':
    unittest.main()
