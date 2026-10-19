#!/usr/bin/env python3
"""Isolated installer proof: tiny synthetic artifacts, no Pi/user state/hosts."""
import importlib.util
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('oqt_installer', Path(__file__).with_name('install-runtime.py'))
installer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(installer)


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='oqt-install-test-')
        self.root = Path(self.temp.name)
        self.upstream = self.root / 'upstream'
        self.snapshot = self.root / 'derivative'
        for tree in (self.upstream, self.snapshot):
            package = tree / 'packages/coding-agent'
            package.mkdir(parents=True)
            (package / 'worker.ts').write_text('patched' if tree == self.snapshot else 'UNPATCHED')
        modules = self.upstream / 'node_modules'
        modules.mkdir()
        (modules / 'workspace').symlink_to('../packages/coding-agent')
        dependency = modules / 'dependency'
        dependency.mkdir()
        (dependency / 'index.js').write_text('dependency')
        (self.snapshot / 'node_modules').symlink_to(modules)
        self.prepared = {'source': str(self.snapshot), 'profile': '/never-copy-owner', 'home': '/never-copy-home'}
        for asset in (*installer.ASSETS, 'node'):
            path = self.root / asset
            path.write_bytes(asset.encode())
            path.chmod(0o700)
            self.prepared[asset] = str(path)
        self.launcher = self.root / 'launcher'
        self.launcher.write_text('#!/bin/sh\nexit 0\n')
        self.launch = self.root / 'launch.py'
        self.launch.write_text('# parent launch\n')
        self.store = self.root / 'store'

    def tearDown(self):
        # Published runtime directories intentionally have no owner write bit.
        for directory, _, _ in os.walk(self.root):
            Path(directory).chmod(0o700)
        self.temp.cleanup()

    def install(self, **kwargs):
        return installer.install(self.prepared, self.launcher, self.store, self.root / 'node',
                                 kwargs.get('extra', [f'launch.py={self.launch}']))

    def test_snapshot_independent_dereferenced_and_idempotent(self):
        info = self.install()
        runtime = Path(info['runtime_root'])
        self.assertFalse((self.store / 'current').exists())
        self.assertEqual(installer.verify_runtime(runtime), info)
        self.assertEqual(self.install(), info)
        self.assertEqual((runtime / 'pi-source/node_modules/workspace/worker.ts').read_text(), 'patched')
        self.assertFalse(any(path.is_symlink() for path in runtime.rglob('*')))
        self.assertEqual(json.loads((runtime / 'runtime.json').read_text()), installer.relative_runtime_paths())
        installer.remove_stage(self.snapshot)
        installer.remove_stage(self.upstream)
        for name in (*installer.ASSETS, 'node'):
            (self.root / name).unlink()
        self.assertEqual(installer.verify_runtime(runtime), info)
        self.assertEqual((runtime / 'bin/node').read_bytes(), b'node')

    def test_bytes_mode_extra_and_symlink_tampering_fail_closed(self):
        for mutation in ('bytes', 'mode', 'extra', 'symlink', 'manifest'):
            with self.subTest(mutation=mutation):
                info = self.install()
                runtime = Path(info['runtime_root'])
                target = runtime / 'bin/hands'
                if mutation == 'bytes':
                    target.chmod(0o700)
                    target.write_bytes(b'TAMPER')
                    target.chmod(0o500)
                elif mutation == 'mode':
                    target.chmod(0o700)
                elif mutation == 'extra':
                    runtime.chmod(0o700)
                    (runtime / 'extra').write_text('extra')
                    runtime.chmod(0o500)
                elif mutation == 'symlink':
                    target.parent.chmod(0o700)
                    target.unlink()
                    target.symlink_to(self.root / 'hands')
                    target.parent.chmod(0o500)
                else:
                    manifest = runtime / 'manifest.json'
                    manifest.chmod(0o600)
                    saved = json.loads(manifest.read_text())
                    saved['files']['bin/hands']['sha256'] = '0' * 64
                    manifest.write_text(json.dumps(saved))
                    manifest.chmod(0o400)
                with self.assertRaises((ValueError, OSError)):
                    installer.verify_runtime(runtime)
                with self.assertRaises((ValueError, OSError)):
                    self.install()
                installer.remove_stage(runtime)

    def test_failed_staging_preserves_current(self):
        first = self.install()
        installer.activate(Path(first['runtime_root']))
        current = os.readlink(self.store / 'current')
        (self.upstream / 'node_modules/broken').symlink_to('missing')
        with self.assertRaises(OSError):
            self.install()
        self.assertEqual(os.readlink(self.store / 'current'), current)
        self.assertEqual(installer.verify_runtime(Path(first['runtime_root'])), first)
        self.assertEqual(list((self.store / 'runtimes').glob('.staging-*')), [])

    def test_cycle_and_special_dependencies_rejected(self):
        modules = self.upstream / 'node_modules'
        (modules / 'loop').symlink_to('.')
        with self.assertRaises(ValueError):
            self.install()
        (modules / 'loop').unlink()
        os.mkfifo(modules / 'fifo')
        with self.assertRaises(ValueError):
            self.install()

    def test_private_directory_symlink_injection_rejected(self):
        elsewhere = self.root / 'elsewhere'
        elsewhere.mkdir(mode=0o700)
        self.store.symlink_to(elsewhere)
        with self.assertRaises(ValueError):
            self.install()
        self.store.unlink()
        self.store.mkdir(mode=0o700)
        (self.store / 'runtimes').symlink_to(elsewhere)
        with self.assertRaises(ValueError):
            self.install()
        self.assertEqual(list(elsewhere.iterdir()), [])

    def test_current_link_injection_rejected(self):
        info = self.install()
        (self.store / 'current').symlink_to('/tmp/untrusted')
        with self.assertRaises(ValueError):
            installer.activate(Path(info['runtime_root']))
        self.assertEqual(os.readlink(self.store / 'current'), '/tmp/untrusted')

    def test_activation_and_launcher_backup(self):
        first = self.install()
        bindir = self.root / 'bin'
        bindir.mkdir(mode=0o700)
        destination = bindir / 'oqt'
        destination.write_bytes(b'old launcher')
        destination.chmod(0o700)
        installer.activate(Path(first['runtime_root']), destination)
        backups = list((self.store / 'launcher-backups').iterdir())
        self.assertEqual(len(backups), 1)
        backup = backups[0] / 'launcher.bytes'
        self.assertEqual(backup.read_bytes(), b'old launcher')
        self.assertTrue(destination.is_symlink())
        self.assertEqual(destination.resolve(), Path(first['launcher']))
        self.launch.write_text('# next version\n')
        second = self.install()
        self.assertNotEqual(first['artifact'], second['artifact'])
        installer.activate(Path(second['runtime_root']), destination)
        self.assertEqual(destination.resolve(), Path(second['launcher']))
        self.assertEqual(backup.read_bytes(), b'old launcher')
        self.assertEqual(list((self.store / 'launcher-backups').iterdir()), backups)
        self.assertEqual(installer.verify_runtime(Path(first['runtime_root'])), first)

    def test_original_symlink_launcher_backup(self):
        info = self.install()
        bindir = self.root / 'bin'
        bindir.mkdir(mode=0o700)
        destination = bindir / 'oqt'
        destination.symlink_to('../launcher')
        installer.activate(Path(info['runtime_root']), destination)
        backup, = (self.store / 'launcher-backups').iterdir()
        self.assertEqual(os.readlink(backup / 'launcher.symlink'), '../launcher')
        self.assertEqual((backup / 'launcher.bytes').read_bytes(), self.launcher.read_bytes())
        metadata = json.loads((backup / 'metadata.json').read_text())
        self.assertEqual(metadata['link_target'], '../launcher')
        self.assertEqual(metadata['path'], str(destination))
        self.launcher.write_text('later user source edits')
        self.assertNotEqual((backup / 'launcher.bytes').read_bytes(), self.launcher.read_bytes())

    def test_namespace_0755_preserved_and_writable_rejected(self):
        self.store.mkdir(mode=0o755)
        self.store.chmod(0o755)
        unknown = self.store / 'unknown-user-file'
        unknown.write_bytes(b'preserve me')
        info = self.install()
        self.assertEqual(self.store.stat().st_mode & 0o777, 0o755)
        self.assertEqual((self.store / 'runtimes').stat().st_mode & 0o777, 0o700)
        self.assertEqual(unknown.read_bytes(), b'preserve me')
        installer.activate(Path(info['runtime_root']))
        self.assertEqual(self.store.stat().st_mode & 0o777, 0o755)
        self.store.chmod(0o775)
        with self.assertRaises(ValueError):
            self.install()

    def test_each_unmanaged_edit_has_unique_private_backup(self):
        info = self.install()
        bindir = self.root / 'bin'
        bindir.mkdir(mode=0o700)
        destination = bindir / 'oqt'
        for content in ('first user launcher', 'later user launcher'):
            destination.unlink(missing_ok=True)
            destination.write_text(content)
            installer.activate(Path(info['runtime_root']), destination)
        backups = list((self.store / 'launcher-backups').iterdir())
        self.assertEqual(len(backups), 2)
        self.assertEqual({(entry / 'launcher.bytes').read_text() for entry in backups},
                         {'first user launcher', 'later user launcher'})
        for backup in backups:
            self.assertEqual(backup.stat().st_mode & 0o777, 0o700)

    def test_launcher_replacement_failure_restores_previous_current_and_command(self):
        original_replace = installer.Path.replace
        for has_current, is_link in ((False, False), (False, True), (True, False), (True, True)):
            with self.subTest(has_current=has_current, is_link=is_link):
                (self.store / 'current').unlink(missing_ok=True)
                first = self.install()
                if has_current:
                    installer.activate(Path(first['runtime_root']))
                self.launch.write_text(f'# next {has_current}\n')
                second = self.install()
                bindir = self.root / f'bin-{has_current}-{is_link}'
                bindir.mkdir(mode=0o700)
                destination = bindir / 'oqt'
                original = bindir / 'original' if is_link else destination
                original.write_text('#!/bin/sh\nprintf old-command')
                original.chmod(0o700)
                if is_link:
                    destination.symlink_to('original')
                previous = os.readlink(self.store / 'current') if has_current else None
                def fail_launcher(path, target):
                    if target == destination:
                        # Pointer must already be usable before replacement.
                        self.assertEqual((self.store / 'current').resolve(), Path(second['runtime_root']))
                        raise OSError('injected launcher replacement failure')
                    return original_replace(path, target)
                with patch.object(installer.Path, 'replace', fail_launcher):
                    with self.assertRaises(OSError):
                        installer.activate(Path(second['runtime_root']), destination)
                self.assertEqual(destination.is_symlink(), is_link)
                self.assertEqual(subprocess.check_output([str(destination)], text=True, timeout=5), 'old-command')
                if previous is None:
                    self.assertFalse((self.store / 'current').is_symlink())
                else:
                    self.assertEqual(os.readlink(self.store / 'current'), previous)
                    self.assertEqual(installer.verify_runtime((self.store / 'current').resolve()), first)

    def test_current_replacement_failure_leaves_unmanaged_launcher_intact(self):
        info = self.install()
        destination = self.root / 'command'
        destination.write_text('original launcher')
        with patch.object(installer.Path, 'replace', side_effect=OSError('pointer failure')):
            with self.assertRaises(OSError):
                installer.activate(Path(info['runtime_root']), destination)
        self.assertEqual(destination.read_text(), 'original launcher')
        self.assertFalse(destination.is_symlink())
        self.assertFalse((self.store / 'current').is_symlink())
        self.assertEqual(list(self.root.glob('.oqt-launcher-*')), [])

    def test_extra_file_escape_rejected(self):
        for relative in ('../escape', '/escape', 'bin/evil', 'pi-source/evil', 'verifier.py'):
            with self.subTest(relative=relative), self.assertRaises(ValueError):
                self.install(extra=[f'{relative}={self.launch}'])
        self.assertFalse((self.store / 'escape').exists())

    def test_missing_launch_fails_before_publication(self):
        with self.assertRaises(ValueError):
            self.install(extra=[])
        self.assertEqual(list((self.store / 'runtimes').iterdir()), [])

    def test_cli_and_standalone_verifier(self):
        prepared_json = self.root / 'prepared.json'
        prepared_json.write_text(json.dumps(self.prepared))
        command = [sys.executable, str(Path(__file__).with_name('install-runtime.py'))]
        completed = subprocess.run([
            *command, 'install', '--prepared-json', str(prepared_json),
            '--launcher', str(self.launcher), '--node', str(self.root / 'node'),
            '--runtime-file', f'launch.py={self.launch}', '--data-root', str(self.store),
        ], check=True, text=True, capture_output=True, timeout=10)
        info = json.loads(completed.stdout)
        runtime = Path(info['runtime_root'])
        completed = subprocess.run([
            sys.executable, str(runtime / 'verifier.py'), 'verify', '--runtime', str(runtime),
        ], check=True, text=True, capture_output=True, timeout=10)
        self.assertEqual(json.loads(completed.stdout), info)
        self.assertFalse((self.store / 'current').exists())
        spec = importlib.util.spec_from_file_location('installed_verifier', runtime / 'verifier.py')
        module = importlib.util.module_from_spec(spec)
        # Disable Python bytecode writing so this test also works when root.
        with patch.object(sys, 'dont_write_bytecode', True):
            spec.loader.exec_module(module)
        self.assertEqual(module.verify_runtime(runtime), info)

    def test_lock_symlink_rejected(self):
        self.store.mkdir(mode=0o700)
        target = self.root / 'outside-lock'
        target.write_text('untouched')
        (self.store / '.install.lock').symlink_to(target)
        with self.assertRaises(OSError):
            self.install()
        self.assertEqual(target.read_text(), 'untouched')
        self.assertEqual(list((self.store / 'runtimes').iterdir()), [])

    @unittest.skipUnless(os.environ.get('OQT_INSTALL_PREPARED_JSON'),
                         'set OQT_INSTALL_PREPARED_JSON to opt into frozen fixture integration')
    def test_real_prepared_dependency_smoke(self):
        prepared = json.loads(Path(os.environ['OQT_INSTALL_PREPARED_JSON']).read_text())
        tools = Path(__file__).resolve().parent
        node = shutil.which('node')
        self.assertIsNotNone(node)
        info = installer.install(
            prepared, tools / 'installed-entry.sh', self.store, Path(node),
            [f'launch.py={tools / "installed-launch.py"}',
             *[f'{name}={tools / name}' for name in
               ('installed-state.py', 'profile_namespace.py', 'preparation_lock.py')]])
        runtime = Path(info['runtime_root'])
        environment = {'PATH': '/usr/bin:/bin', 'HOME': str(self.root), 'PI_OFFLINE': '1',
                       'PYTHONDONTWRITEBYTECODE': '1'}
        completed = subprocess.run(
            [info['node'], '--import', str(runtime / 'pi-source/packages/coding-agent/src/experimental/source-resolver.ts'),
             '--input-type=module', '-e',
             'await import("@earendil-works/pi-ai"); await import("proper-lockfile"); console.log("copied-dependencies-ok")'],
            cwd=runtime / 'pi-source', env=environment, check=True,
            text=True, capture_output=True, timeout=90)
        self.assertIn('copied-dependencies-ok', completed.stdout)
        completed = subprocess.run([str(runtime / 'oqt'), '--runtime-info'],
                                   env=environment, check=True, text=True,
                                   capture_output=True, timeout=90)
        self.assertEqual(json.loads(completed.stdout)['runtime'], str(runtime))
        self.assertFalse((self.store / 'current').is_symlink())
        self.assertEqual(installer.verify_runtime(runtime), info)

    def test_publish_failure_preserves_current(self):
        info = self.install()
        installer.activate(Path(info['runtime_root']))
        current = os.readlink(self.store / 'current')
        self.launch.write_text('# new\n')
        with patch.object(installer.Path, 'rename', side_effect=OSError('failed publication')):
            with self.assertRaises(OSError):
                self.install()
        self.assertEqual(os.readlink(self.store / 'current'), current)
        self.assertEqual(list((self.store / 'runtimes').glob('.staging-*')), [])


if __name__ == '__main__':
    unittest.main()
