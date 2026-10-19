#!/usr/bin/env python3
"""Isolated state/launch policy regressions; actual bundle proof is separate."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location('installed_state', TOOLS/'installed-state.py')
state = importlib.util.module_from_spec(spec)
spec.loader.exec_module(state)


class InstalledStateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='oqt-installed-state-')
        self.root = Path(self.temp.name)
        self.work = self.root/'workspace'; self.work.mkdir()
        self.owner = self.root/'owner'; self.owner.mkdir()
        self.ipc_temp = tempfile.TemporaryDirectory(prefix='i', dir='/tmp')
        self.ipc = Path(self.ipc_temp.name)
        self.home = self.root/'state'; self.home.mkdir(mode=0o700)
        self.artifact = 'ab'*32

    def tearDown(self):
        self.ipc_temp.cleanup()
        self.temp.cleanup()

    def prepare(self, **kwargs):
        return state.prepare_state(self.root/'runtime', self.artifact, kwargs.get('work', self.work),
                                   'coding', kwargs.get('owner', self.owner), self.home, self.ipc)

    def test_persistent_alias_reopen_and_owner_isolation(self):
        first = self.prepare()
        self.assertEqual(first, self.prepare())
        self.assertTrue(Path(first['runtime_root']).is_symlink())
        self.assertTrue(Path(first['profile']).resolve().is_relative_to(self.home))
        other = self.root/'other-owner'; other.mkdir()
        self.assertNotEqual(first['profile'], self.prepare(owner=other)['profile'])
        alias = self.root/'owner-alias'; alias.symlink_to(self.owner)
        self.assertEqual(first, self.prepare(owner=alias))

    def test_preserves_existing_namespace_permissions_and_unknown_files(self):
        namespace = self.home/'oqt'; namespace.mkdir(mode=0o755)
        unknown = namespace/'operator-owned'; unknown.write_text('preserve')
        self.prepare()
        self.assertEqual(unknown.read_text(), 'preserve')
        self.assertEqual(namespace.stat().st_mode & 0o777, 0o755)

    def test_different_state_homes_have_distinct_aliases(self):
        first = self.prepare()
        other = self.root/'other-state'
        second = state.prepare_state(self.root/'runtime', self.artifact, self.work,
                                     'coding', self.owner, other, self.ipc)
        self.assertNotEqual(first['runtime_root'], second['runtime_root'])
        self.assertEqual(first, self.prepare())

    def test_alias_collision_fails_closed(self):
        state.ipc_alias(self.home, self.artifact, self.ipc).symlink_to(self.work)
        with self.assertRaisesRegex(RuntimeError, 'IPC alias'):
            self.prepare()

    def test_state_symlink_and_public_state_rejected(self):
        (self.home/'oqt').symlink_to(self.work)
        with self.assertRaisesRegex(RuntimeError, 'state namespace'):
            self.prepare()
        (self.home/'oqt').unlink()
        namespace = self.home/'oqt'; namespace.mkdir(mode=0o700)
        versions = namespace/'runtimes'; versions.mkdir(mode=0o777); versions.chmod(0o777)
        with self.assertRaisesRegex(RuntimeError, 'private'):
            self.prepare()

    def test_binding_symlink_never_writes_target(self):
        result = self.prepare()
        binding = Path(result['profile'])/'work-directory.json'
        binding.unlink()
        protected = self.root/'must-not-write'; protected.write_text('untouched')
        binding.symlink_to(protected)
        with self.assertRaises(OSError):
            self.prepare()
        self.assertEqual(protected.read_text(), 'untouched')

    def test_invalid_identity_rejected_before_namespace_creation(self):
        self.artifact = '../escape'
        with self.assertRaisesRegex(RuntimeError, 'identity'):
            self.prepare()
        self.assertFalse((self.home/'oqt').exists())

    def test_installed_launcher_allows_source_and_denies_protected_ancestors(self):
        store = self.root/'install'; runtime = store/'runtimes'/self.artifact
        runtime.mkdir(parents=True)
        shutil.copyfile(TOOLS/'installed-launch.py', runtime/'launch.py')
        shutil.copyfile(TOOLS/'installed-state.py', runtime/'installed-state.py')
        (runtime/'install-runtime.py').write_text('def verify_runtime(root): pass\n')
        (runtime/'pi-source').mkdir()
        (runtime/'bin').mkdir()
        shutil.copyfile('/usr/bin/true', runtime/'bin/node')
        (runtime/'runtime.json').write_text(json.dumps({'source': 'pi-source', 'node': 'bin/node'}))
        env = {**os.environ, 'XDG_STATE_HOME': str(self.home)}
        def run(*args):
            return subprocess.run(['/usr/bin/python3', '-I', str(runtime/'launch.py'), *args],
                                  env=env, text=True, capture_output=True, timeout=5)
        info = run('--runtime-info')
        self.assertEqual(info.returncode, 0, info.stderr)
        self.assertEqual(json.loads(info.stdout)['runtime'], str(runtime))
        self.assertFalse((self.home/'oqt').exists())
        for forbidden in (store, store/'runtimes', self.root):
            result = run('--workdir', str(forbidden))
            self.assertEqual(result.returncode, 2)
            self.assertIn('overlaps protected', result.stderr)
        # Source checkout is a sibling of installation, so it passes overlap
        # policy and reaches the later missing-owner check (not a launch).
        result = run('--workdir', str(self.work), '--owner-profile', str(self.root/'missing-owner'))
        self.assertEqual(result.returncode, 2)
        self.assertNotIn('overlaps protected', result.stderr)


if __name__ == '__main__':
    unittest.main()
