#!/usr/bin/env python3
"""`!cmd` / `!!cmd` in the real oqt TUI: the operator's own command runs in the
confined hands without a prompt and without the model; `!cmd` output joins the
next message, `!!cmd` output does not; protected roots stay unreachable."""
import json
import os
from pathlib import Path
import re
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world
from tui_test import Terminal

CSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)")

with coding_world(lambda request: ('text', 'bang-reply'), 'oqt-bang-') as (provider, owner, work):
    t = Terminal(['--owner-profile', str(owner), '--workdir', str(work)], env=dict(os.environ))
    t.deadline = time.monotonic() + 90
    try:
        t.expect('oqt coding')
        m = len(t.raw)
        t.send('!echo bang-visible > made.txt; cat made.txt\r')
        t.expect('durably recorded for this Session', m)
        t.expect('bang-visible', m)
        assert (work / 'made.txt').read_text().strip() == 'bang-visible', 'real effect in the workspace'
        m = len(t.raw)
        t.send('!!echo bang-silent\r')
        t.expect('bang-silent', m)
        m = len(t.raw)
        t.send(f'!ls -A {owner}; echo listed-owner\r')
        t.expect('listed-owner', m)
        shown = CSI.sub(b'', t.raw[m:]).decode('utf8', 'replace')
        assert 'auth.json' not in shown and 'settings.json' not in shown, 'owner profile stays masked'
        screen = CSI.sub(b'', t.raw).decode('utf8', 'replace')
        assert 'Do you want to proceed?' not in screen, 'the operator is never asked about their own command'
        assert provider.calls == [], 'the model is not involved'
        # A concurrently attached TUI must inspect the same worker-owned result.
        peer = Terminal(['--continue', '--owner-profile', str(owner), '--workdir', str(work)], env=dict(os.environ))
        peer.deadline = time.monotonic() + 90
        try:
            peer.expect('oqt coding')
            marker = len(peer.raw)
            peer.send('/shell-results\r')
            peer.expect('bang-visible', marker)
            assert provider.calls == [], 'a second client reading receipts must not call inference'
        finally:
            peer.close('bang-peer')
        # A different client process must recover context without any local queue.
        t.close('bang-before-reopen')
        time.sleep(1.5)
        t = Terminal(['--continue', '--owner-profile', str(owner), '--workdir', str(work)], env=dict(os.environ))
        t.deadline = time.monotonic() + 90
        t.expect('oqt coding')
        m = len(t.raw)
        t.send('/shell-results\r')
        t.expect('bang-visible', m)
        assert provider.calls == [], 'receipt reload/inspection must not initiate inference'
        m = len(t.raw)
        t.send('what happened?\r')
        t.expect('bang-reply', m)
        user = json.dumps([x for x in provider.calls[-1]['messages'] if x['role'] == 'user'])
        assert 'bang-visible' in user and 'Operator shell result' in user, '!cmd durable context survives client reopen'
        assert 'bang-silent' not in user, '!!cmd output stays out of context'
        assert 'auth.json' not in user, 'nothing from the owner profile reaches the model'
        print('PASS bang: !cmd runs confined without prompt/model and persists across client reopen; '
              '!!cmd stays private; protected roots unreachable', flush=True)
    finally:
        t.close('bang')
