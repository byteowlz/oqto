#!/usr/bin/env python3
"""Opt-in installed-runtime startup proof with disposable owner/provider/state.

OQT_INSTALLED_TEST_RUNTIME names a staged runtime (never activated by this test).
Measures, from launcher exec, the first editable frame (startup notice visible
and a typed draft echoed) for: a fresh state home (full content verification)
and a second launch (verification receipt, warm host). The first launch queues
one message before the host exists; it must reach the provider exactly once.
Retires only its own positively identified fixture host.
"""
import hashlib
import json
import os
from pathlib import Path
import signal
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tui_test
from coding_fixture import coding_world

RUNTIME = os.environ.get('OQT_INSTALLED_TEST_RUNTIME')
if not RUNTIME:
    print('SKIP: OQT_INSTALLED_TEST_RUNTIME must name a staged complete runtime')
    sys.exit(0)
runtime = Path(RUNTIME).resolve(strict=True)
samples = int(os.environ.get('OQT_EARLY_SAMPLES', '3'))
entry = Path(tempfile.mkdtemp(prefix='oqt-installed-entry-'))
(entry/'oqt').symlink_to(runtime/'oqt')
tui_test.ROOT = entry  # Terminal executes ROOT/oqt -> staged runtime entry


def retire(state_root, owner):
    for manifest in state_root.glob('p/*/host.json'):
        info = json.loads(manifest.read_text())
        try:
            descriptor = os.pidfd_open(info['pid'])
        except ProcessLookupError:
            continue
        try:
            environment = Path(f'/proc/{info["pid"]}/environ').read_bytes().split(b'\0')
            command = Path(f'/proc/{info["pid"]}/cmdline').read_bytes()
            if (f'OQT_OWNER_AGENT_DIR={owner}'.encode() not in environment
                    or str(runtime).encode() not in command or b'/host.ts' not in command):
                raise AssertionError('Refusing to signal an unproven fixture host')
            signal.pidfd_send_signal(descriptor, signal.SIGTERM)
        except FileNotFoundError:
            pass
        finally:
            os.close(descriptor)
    deadline = time.monotonic()+10
    while list(state_root.glob('p/*/host.json')) and time.monotonic() < deadline:
        time.sleep(.1)
    assert not list(state_root.glob('p/*/host.json')), 'Fixture host did not retire'


def first_frame(owner, work, env, label, draft):
    terminal = tui_test.Terminal(['--owner-profile', str(owner), '--workdir', str(work)], cwd=work, env=env)
    terminal.deadline = time.monotonic()+120
    try:
        terminal.expect('Loading oqt')
        painted = time.monotonic()-terminal.started
        terminal.send(draft)
        terminal.expect(draft)
        editable = time.monotonic()-terminal.started
        return terminal, painted, editable
    except BaseException:
        terminal.close(label)
        raise


results = {'full': [], 'receipt': [], 'attach_reply': [], 'queued_path': []}
try:
    for sample in range(samples):
        with coding_world(lambda request: ('text', 'installed-early-reply'), 'oqt-installed-early-') as (provider, owner, work):
            state_home = work.parent/'state'
            state_root = state_home/'oqt/runtimes'/runtime.name
            # Node writes the compile cache while shutting down; keep it out of
            # the fixture tree so teardown cannot race a late write.
            cache = Path(tempfile.mkdtemp(prefix='oqt-installed-cache-', dir='/var/tmp'))
            env = {**os.environ, 'XDG_STATE_HOME': str(state_home), 'OQT_HOST_IDLE_SECONDS': '30', 'OQT_DEBUG': '1',
                   'NODE_COMPILE_CACHE': str(cache)}
            try:
                # Fresh state: no receipt, full verification, cold host.
                terminal, painted, editable = first_frame(owner, work, env, f'installed-early-full-{sample}', 'queued-installed')
                try:
                    mark = len(terminal.raw)
                    terminal.send('\r')
                    terminal.expect('installed-early-reply', mark)
                    reply = time.monotonic()-terminal.started
                    queued = 'Queued' in tui_test.CSI.sub(b'', terminal.raw[mark:]).decode('utf8', 'replace')
                    results['queued_path'].append(queued)
                    deadline = time.monotonic()+1.5
                    while time.monotonic() < deadline:
                        time.sleep(.1)
                    assert len(provider.calls) == 1, f'expected one dispatch, got {len(provider.calls)}'
                    assert 'queued-installed' in json.dumps(provider.calls[0])
                finally:
                    terminal.close(f'installed-early-full-{sample}')
                results['full'].append((painted, editable))
                results['attach_reply'].append(reply)
                # Second launch: existing state, lingering warm host.
                terminal, painted, editable = first_frame(owner, work, env, f'installed-early-receipt-{sample}', 'second-draft')
                terminal.close(f'installed-early-receipt-{sample}')
                results['receipt'].append((painted, editable))
                # The launch path no longer hashes the runtime; the explicit
                # full check still passes on the same tree (ADR-0072).
                full = subprocess.run([str(entry/'oqt'), 'verify'], env=env, text=True,
                                      capture_output=True, timeout=120)
                assert full.returncode == 0, full.stderr
                assert len(provider.calls) == 1, 'second launch dispatched without Enter'
            finally:
                retire(state_root, owner)
                key = hashlib.sha256(os.fsencode(state_root.resolve())).hexdigest()[:10]
                alias = Path('/var/tmp')/f'qi{os.getuid()}-{key}'
                if alias.is_symlink() and os.readlink(alias) == str(state_root):
                    alias.unlink()
                shutil.rmtree(cache, ignore_errors=True)
finally:
    (entry/'oqt').unlink()
    entry.rmdir()


def summary(values):
    return f'p50 {statistics.median(values):.2f}s max {max(values):.2f}s (n={len(values)})'


print('PASS installed early editor: draft typed during startup reached the provider exactly once;',
      f"queued before attach in {sum(results['queued_path'])}/{len(results['queued_path'])} samples")
print('  cold launch (fresh state)     — first frame', summary([v[0] for v in results['full']]),
      '| editable', summary([v[1] for v in results['full']]))
print('  warm launch (existing state)  — first frame', summary([v[0] for v in results['receipt']]),
      '| editable', summary([v[1] for v in results['receipt']]))
print('  cold launch -> startup draft answered', summary(results['attach_reply']))
