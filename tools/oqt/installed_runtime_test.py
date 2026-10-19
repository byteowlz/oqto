#!/usr/bin/env python3
"""Opt-in real installed-runtime proof with a disposable owner/provider/state."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time
import unittest
from coding_fixture import coding_world, snapshot

RUNTIME = os.environ.get('OQT_INSTALLED_TEST_RUNTIME')
REPO = Path(__file__).resolve().parents[2]


@unittest.skipUnless(RUNTIME, 'OQT_INSTALLED_TEST_RUNTIME must name a staged complete runtime')
class InstalledRuntimeTests(unittest.TestCase):
    def test_actual_confined_coding_from_source_checkout(self):
        runtime = Path(RUNTIME).resolve(strict=True)
        with coding_world(lambda request: ('text', 'oqt-coding-ready.'), 'oqt-installed-proof-') as (provider, owner, work):
            before = snapshot(owner)
            state_home = work.parent/'persistent-state'
            env = {**os.environ, 'XDG_STATE_HOME': str(state_home), 'OQT_HOST_IDLE_SECONDS': '1'}
            state_root = state_home/'oqt/runtimes'/runtime.name
            try:
                entrypoint = os.environ.get('OQT_INSTALLED_TEST_EXECUTABLE', str(runtime/'oqt'))
                completed = subprocess.run([entrypoint, '--workdir', str(REPO),
                                            '--owner-profile', str(owner), '--smoke'],
                                           cwd=REPO, env=env, text=True, capture_output=True, timeout=90)
                if completed.returncode:
                    logs = '\n'.join(path.read_text()[-12000:] for path in state_root.glob('p/*/*.log'))
                    self.fail(f'{completed.stdout}\n{completed.stderr}\n{logs}')
                self.assertIn('oqt coding smoke PASS', completed.stdout)
                self.assertEqual(len(provider.calls), 1)
                self.assertEqual(snapshot(owner), before)
                self.assertTrue(list(state_root.glob('p/*/sessions/*')))
                self.assertIn(str(REPO), json.dumps(provider.calls))
            finally:
                # Retire only this positively identified fixture host, if linger
                # has not already done so. Never signal the user's live hosts.
                for manifest in state_root.glob('p/*/host.json'):
                    info = json.loads(manifest.read_text())
                    try:
                        descriptor = os.pidfd_open(info['pid'])
                    except ProcessLookupError:
                        continue
                    try:
                        environment = Path(f'/proc/{info["pid"]}/environ').read_bytes().split(b'\0')
                        command = Path(f'/proc/{info["pid"]}/cmdline').read_bytes()
                        if f'OQT_OWNER_AGENT_DIR={owner}'.encode() not in environment or str(runtime).encode() not in command or b'/host.ts' not in command:
                            raise AssertionError('Refusing to signal an unproven fixture host')
                        signal.pidfd_send_signal(descriptor, signal.SIGTERM)
                    except FileNotFoundError:
                        pass
                    finally:
                        os.close(descriptor)
                deadline = time.monotonic()+8
                while list(state_root.glob('p/*/host.json')) and time.monotonic() < deadline:
                    time.sleep(.1)
                if list(state_root.glob('p/*/host.json')):
                    raise AssertionError('Fixture host did not retire')
                # Namespace is unique to this disposable state home.
                import hashlib
                key = hashlib.sha256(os.fsencode(state_root.resolve())).hexdigest()[:10]
                alias = Path('/var/tmp')/f'qi{os.getuid()}-{key}'
                if alias.is_symlink() and os.readlink(alias) == str(state_root):
                    alias.unlink()


if __name__ == '__main__':
    unittest.main()
