#!/usr/bin/env python3
"""Real TUI/PTY: the model asks for an outside path, the operator approves in the
dialog, the guard mounts it on fresh hands, and the follow-up read is approved
separately and sees the file."""
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world, snapshot, tool_results
from tui_test import Terminal

state = {}


def script(request):
    results = json.dumps(tool_results(request))
    outside = state['outside']
    if 'Granted read access' not in results:
        return ('tool', 'request_access', {'path': str(outside), 'access': 'read',
                                           'reason': 'compare with the shared library'})
    if 'outside-data-5513' not in results:
        return ('tool', 'read', {'path': str(outside / 'data.txt')})
    return ('text', 'access-complete')


with coding_world(script, 'oqt-access-') as (provider, owner, work):
    outside = work.parent / 'shared-library'
    outside.mkdir()
    (outside / 'data.txt').write_text('outside-data-5513')
    state['outside'] = outside
    original = snapshot(owner)
    terminal = Terminal(['--coding', '--owner-profile', str(owner), '--workdir', str(work)])
    terminal.deadline = time.monotonic() + 90
    try:
        terminal.expect('oqt coding')
        marker = len(terminal.raw)
        terminal.send('Compare with the shared library.\r')
        terminal.expect(f'Access request · read {outside}', marker)
        terminal.expect('Reason: compare with the shared library', marker)
        terminal.expect('→ Deny', marker)
        screen = terminal.raw[marker:].decode('utf8', 'replace')
        assert 'Allow once' not in screen, 'resource grants are Session-scoped only'
        terminal.send('\x1b[A\r')  # Allow for this Session (1 hour)
        marker = terminal.expect('Permission applied', marker)
        terminal.expect('Coding permission · read', marker)
        terminal.expect('→ Deny', marker)
        terminal.send('\x1b[A\x1b[A\r')  # Allow once
        marker = terminal.expect('Permission applied', marker)
        terminal.expect('access-complete', marker)
        assert 'outside-data-5513' in json.dumps(tool_results(provider.calls[-1]))
        assert snapshot(owner) == original
        print('PASS access TUI: operator-approved outside path mounted read-only on fresh hands; '
              'follow-up tool call approved separately', flush=True)
    finally:
        terminal.close('coding-access')
