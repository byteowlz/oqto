#!/usr/bin/env python3
"""Real PTY worker router proof against disposable owning profile/provider only."""
import hashlib
import json
import os
from pathlib import Path
import re
import select
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world
from tui_test import Terminal, CSI


def runtime_inputs():
    # prepare.py namespaces catalogs by runtime artifact. Detect cross-lane edits
    # before asserting reopen; otherwise a new catalog masquerades as data loss.
    root = Path(__file__).resolve().parents[2]
    lane = root / 'tools/oqt'
    paths = [root / 'oqt', lane / 'prepare.py', lane / 'build-assets.py',
             lane / 'preparation_lock.py', lane / 'profile_namespace.py',
             lane / 'source_patches.py', lane / 'hands-proxy.mjs']
    paths += list(lane.glob('*.ts'))
    paths += [path for path in (lane / 'plugin').rglob('*') if path.is_file()]
    result = {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}
    for path in [root / 'backend/target/debug/examples' / name for name in
                 ['permission_carrier', 'oqt_hands', 'oqt_shim', 'oqt_brain']]:
        meta = path.stat()
        result[str(path.relative_to(root))] = (meta.st_ino, meta.st_size, meta.st_mtime_ns)
    return result


def user_text(request):
    content = next(m['content'] for m in reversed(request['messages']) if m['role'] == 'user')
    if isinstance(content, list):
        return ''.join(part.get('text', '') for part in content)
    return content


def script(request):
    return ('text', 'answer-' + user_text(request).replace(' ', '_'))


def drain(t, seconds=.5):
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        if select.select([t.master], [], [], .05)[0]:
            data = os.read(t.master, 65536)
            t.raw.extend(data)
            if b'\x1b[6n' in data:
                os.write(t.master, b'\x1b[1;1R')


def prompt(t, text):
    marker = len(t.raw)
    t.send(text + '\r')
    t.expect('answer-' + text.replace(' ', '_'), marker)
    drain(t, .15)


def tree(t, conversation):
    marker = len(t.raw)
    t.send('/tree\r')
    t.expect('Conversations', marker)
    # Exactly two conversations in this fixture. Match the displayed native ID;
    # use arrows rather than fuzzy text ("1" also matches the child ID "16").
    t.expect(f'Conversation {conversation}', marker)
    t.send(('\x1b[A' if conversation == 1 else '\x1b[B') + '\r')
    t.expect('alpha later' if conversation == 1 else 'branch delta', marker)
    drain(t, .4)


def request_for(provider, text):
    matches = [request for request in provider.calls if user_text(request) == text]
    assert matches, (text, provider.calls)
    return matches[-1]


def assert_context(provider, text, present=(), absent=(), model=None):
    request = request_for(provider, text)
    context = json.dumps(request['messages'])
    for value in present:
        assert value in context, (text, 'missing', value, context)
    for value in absent:
        assert value not in context, (text, 'unexpected', value, context)
    if model:
        assert request['model'] == model, request


with coding_world(script, 'oqt-tree-') as (provider, owner, work):
    # Only fixture-owned configuration is changed. No real profile access.
    config = json.loads((owner / 'models.json').read_text())
    template = config['providers']['fixture']['models'][0]
    config['providers']['fixture']['models'].append(dict(template, id='owner-alt', name='Alternate Fixture'))
    (owner / 'models.json').write_text(json.dumps(config))
    args = ['--owner-profile', str(owner), '--workdir', str(work)]
    frozen_inputs = runtime_inputs()
    t = Terminal(args)
    second = None
    t.deadline = time.monotonic() + 100
    try:
        t.expect('oqt coding')
        prompt(t, 'alpha origin')
        prompt(t, 'alpha later')
        marker = len(t.raw)
        t.send('/branch\r')
        t.expect('Branch at entry (inclusive)', marker)
        t.send('pi.user')
        drain(t, .5)
        labels = CSI.sub(b'', t.raw[marker:]).decode('utf8', 'replace')
        user_ids = [int(value) for value in re.findall(r'Entry (\d+): pi\.user', labels)]
        assert len(set(user_ids)) >= 2, labels[-6000:]
        first_user = min(user_ids)
        t.send('\x1b')  # first Esc clears search
        drain(t, .2)
        t.send('\x1b')  # second Esc closes the picker
        drain(t, .3)
        marker = len(t.raw)
        t.send(f'/branch {first_user}\r')
        t.expect('Conversation ', marker)
        drain(t, .3)
        status = CSI.sub(b'', t.raw[marker:]).decode('utf8', 'replace')
        ids = [int(value) for value in re.findall(r'Conversation (\d+)', status)]
        assert ids, status[-4000:]
        child = ids[-1]
        assert child != 1, status[-4000:]
        prompt(t, 'branch delta')
        assert_context(provider, 'branch delta', present=['alpha origin'], absent=['alpha later'])
        marker = len(t.raw)
        t.send('/model fixture/owner-alt\r')
        t.expect('Model: fixture/owner-alt', marker)
        prompt(t, 'branch model')
        assert_context(provider, 'branch model', absent=['alpha later'], model='owner-alt')
        marker = len(t.raw)
        t.send('!printf branch-shell-token\r')
        t.expect('branch-shell-token', marker)
        drain(t, .4)
        prompt(t, 'branch after shell')
        assert_context(provider, 'branch after shell', present=['branch-shell-token'], model='owner-alt')
        second = Terminal(['--continue', *args])
        second.deadline = time.monotonic() + 100
        second.expect('branch after shell')
        picker_marker = len(second.raw)
        second.send('/model\r')
        second.expect('Select model', picker_marker)
        tree(t, 1)
        prompt(t, 'root continuation')
        assert_context(provider, 'root continuation', present=['alpha later'], absent=['branch delta', 'branch-shell-token'], model='owner-model')
        picker_marker = len(second.raw)
        second.send('\r')
        # The stock server redacts worker exceptions to its generic error banner;
        # the native router regression asserts the exact stale-target error.
        second.expect('Error: Internal server error', picker_marker)
        retry_marker = len(second.raw)
        second.send('/model fixture/owner-model\r')
        second.expect('Model: fixture/owner-model', retry_marker)
        prompt(second, 'second client root')
        assert_context(provider, 'second client root', present=['alpha later'], absent=['branch delta', 'branch-shell-token'], model='owner-model')
        observer_marker = len(second.raw)
        tree(t, child)
        second.expect('branch delta', observer_marker)
        prompt(t, 'branch selected')
        assert_context(provider, 'branch selected', present=['branch delta', 'branch-shell-token'], absent=['alpha later'], model='owner-alt')
        prompt(second, 'second client branch')
        assert_context(provider, 'second client branch', present=['branch delta', 'branch-shell-token'], absent=['alpha later'], model='owner-alt')
        print('PASS two TUI clients: shared transcript/model rebinding, stale open model picker rejected, input targets exact selected branch', flush=True)
        print('PASS real TUI /branch + /tree: transcript, input, model settings and ! receipt destination', flush=True)
    finally:
        if second is not None:
            second.close('tree-router-second')
        t.close('tree-router')
    # Let fixture host/worker close; --continue must restore SQLite selection.
    time.sleep(2)
    current_inputs = runtime_inputs()
    changed = sorted(path for path in frozen_inputs.keys() | current_inputs.keys()
                     if frozen_inputs.get(path) != current_inputs.get(path))
    assert not changed, f'Runtime artifact drift during TUI proof; freeze all lanes then rerun: {changed}'
    reopened = Terminal(['--continue', *args])
    reopened.deadline = time.monotonic() + 100
    try:
        reopened.expect('oqt coding')
        reopened.expect('branch selected')
        prompt(reopened, 'branch reopened')
        assert_context(provider, 'branch reopened', present=['branch delta', 'branch-shell-token'], absent=['alpha later'], model='owner-alt')
        print('PASS real TUI reopen: persisted selected native conversation, transcript, model and receipt context', flush=True)
    finally:
        reopened.close('tree-router-reopen')
