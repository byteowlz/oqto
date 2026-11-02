#!/usr/bin/env python3
"""Real source-runtime editor while host creation is held at its actual lock.

Proves: early typing, one queued startup message dispatched automatically and
exactly once after attachment, Esc cancellation without dispatch, prompt exit
while blocked, and startup failure that keeps text and sends nothing.
No sleep-based readiness injection, user profile, or live user host is used.
Does NOT prove installed-runtime verification ordering (see installed tests).
"""
import json
import os
from pathlib import Path
import subprocess
import time

from coding_fixture import coding_world
from tui_test import Terminal

ROOT = Path(__file__).resolve().parents[2]
SOURCE = os.environ.get('OQT_PI_SOURCE', str(Path.home()/'.local/share/oqt/pi-source'))
os.environ['OQT_HOST_IDLE_SECONDS'] = '1'


def prepare(work, owner):
    prepared = json.loads(subprocess.check_output([
        'python3', str(ROOT/'tools/oqt/prepare.py'), SOURCE, str(work), 'coding', str(owner)
    ], text=True))
    return Path(prepared['profile'])


def launch(owner, work, env=None, label='early'):
    terminal = Terminal(['--owner-profile', str(owner), '--workdir', str(work)], env=env)
    terminal.deadline = time.monotonic()+90
    terminal.label = label
    return terminal


def settle(provider, expected, seconds=2.0):
    deadline = time.monotonic()+seconds
    while time.monotonic() < deadline:
        assert len(provider.calls) <= expected, f'unexpected dispatch: {len(provider.calls)} > {expected}'
        time.sleep(.1)
    assert len(provider.calls) == expected, f'expected {expected} provider calls, got {len(provider.calls)}'


with coding_world(lambda request: ('text', 'early-editor-reply'), 'oqt-early-editor-') as (provider, owner, work):
    profile = prepare(work, owner)
    lock = profile/'host.lock'

    # 1. Exit while the host lock is held: no host is created.
    lock.mkdir(mode=0o700)
    exiting = launch(owner, work, label='early-editor-exit-held-host')
    try:
        exiting.expect('Loading oqt')
        exiting.send('\x04')
        exiting.process.wait(timeout=3)
        assert exiting.process.returncode == 0, 'Early editor exit failed'
        assert not (profile/'host.json').exists(), 'Exit started a host through the held lock'
    finally:
        exiting.close('early-editor-exit-held-host')

    # 2. Queue before the host exists; dispatch exactly once after attach.
    terminal = launch(owner, work)
    try:
        terminal.expect('Loading oqt')
        terminal.send('draft-before-host')
        terminal.expect('draft-before-host')
        mark = len(terminal.raw)
        terminal.send('\r')
        terminal.expect('Queued — sends when oqt is ready', mark)
        settle(provider, 0, 1.0)
        assert not (profile/'host.json').exists(), 'Host started through held creation lock'
        lock.rmdir()
        terminal.expect('early-editor-reply', mark)
        settle(provider, 1)
        request = json.dumps(provider.calls[0])
        assert 'draft-before-host' in request, 'Queued text was not the dispatched prompt'
        print('PASS queued startup message: dispatched automatically, exactly once, after attach', flush=True)
    finally:
        if lock.exists():
            lock.rmdir()
        terminal.close('early-editor-queued-dispatch')

    # 3. Esc withdraws the queued message: restored to the editor, never sent.
    lock.mkdir(mode=0o700)
    terminal = launch(owner, work)
    try:
        terminal.expect('Loading oqt')
        terminal.send('cancel-me-please')
        mark = len(terminal.raw)
        terminal.send('\r')
        terminal.expect('Queued — sends when oqt is ready', mark)
        mark = len(terminal.raw)
        terminal.send('\x1b')
        terminal.expect('text restored to the editor', mark)
        terminal.expect('cancel-me-please', mark)
        lock.rmdir()
        terminal.expect('oqt coding')
        settle(provider, 1, 3.0)
        print('PASS Esc cancellation: text restored, nothing dispatched after attach', flush=True)
    finally:
        if lock.exists():
            lock.rmdir()
        terminal.close('early-editor-cancel')

with coding_world(lambda request: ('text', 'must-not-run'), 'oqt-early-failure-') as (provider, owner, work):
    # 4. Startup failure: invalid policy stops the host; queued text stays, nothing sent.
    profile = prepare(work, owner)
    policy = work.parent/'invalid-sandbox.toml'
    policy.write_text('this is = = not toml\n')
    env = {**os.environ, 'OQT_SANDBOX_CONFIG': str(policy)}
    terminal = launch(owner, work, env=env)
    try:
        terminal.expect('Loading oqt')
        terminal.send('keep-after-failure')
        mark = len(terminal.raw)
        terminal.send('\r')
        # Enter may land before or after the failure; either way nothing is sent.
        terminal.expect('Startup failed; nothing was sent')
        terminal.expect('keep-after-failure', mark)
        settle(provider, 0)
        print('PASS startup failure: queued text restored, zero provider calls', flush=True)
    finally:
        terminal.close('early-editor-failure')
