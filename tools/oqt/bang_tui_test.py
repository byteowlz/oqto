#!/usr/bin/env python3
"""`!cmd` / `!!cmd` in the real oqt TUI: the operator's own command runs in the
confined hands without a prompt and without the model; `!cmd` output joins the
next message, `!!cmd` output does not; protected roots stay unreachable."""
import json
from pathlib import Path
import re
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world
from tui_test import Terminal

CSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)")

with coding_world(lambda request: ('text', 'bang-reply'), 'oqt-bang-') as (provider, owner, work):
    t = Terminal(['--owner-profile', str(owner), '--workdir', str(work)])
    t.deadline = time.monotonic() + 90
    try:
        t.expect('oqt coding')
        m = len(t.raw)
        t.send('!echo bang-visible > made.txt; cat made.txt\r')
        t.expect('added to your next message', m)
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
        m = len(t.raw)
        t.send('what happened?\r')
        t.expect('bang-reply', m)
        user = json.dumps([x for x in provider.calls[-1]['messages'] if x['role'] == 'user'])
        assert 'bang-visible' in user and 'Ran `echo bang-visible' in user, '!cmd output joins the next message'
        assert 'bang-silent' not in user, '!!cmd output stays out of context'
        assert 'auth.json' not in user, 'nothing from the owner profile reaches the model'
        print('PASS bang: !cmd runs confined without prompt/model and joins the next message; '
              '!!cmd stays private; protected roots unreachable', flush=True)
    finally:
        t.close('bang')
