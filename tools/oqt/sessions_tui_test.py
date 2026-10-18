#!/usr/bin/env python3
"""Classic-pi session commands in the real oqt TUI: /new starts a fresh Session
(new transcript, new model context), /resume switches back to the earlier one
with its history, and both Sessions persist."""
import json
from pathlib import Path
import re
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world, last_user_text
from tui_test import Terminal

CSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)")


def script(request):
    users = [m for m in request['messages'] if m['role'] == 'user']
    return ('text', f"turns-seen-{len(users)}-{json.loads(last_user_text(request))['content'][:12].replace(' ', '_')}")


with coding_world(script, 'oqt-sessions-') as (provider, owner, work):
    t = Terminal(['--owner-profile', str(owner), '--workdir', str(work)])
    t.deadline = time.monotonic() + 120
    try:
        t.expect('oqt coding')
        m = len(t.raw)
        t.send('alpha first\r')
        t.expect('turns-seen-1-alpha_first', m)
        m = len(t.raw)
        t.send('alpha second\r')
        t.expect('turns-seen-2-alpha_second', m)
        # /new: fresh model context (one user turn only).
        m = len(t.raw)
        t.send('/new\r')
        t.expect('Session ', m)
        m = len(t.raw)
        t.send('beta first\r')
        t.expect('turns-seen-1-beta_first', m)
        print('PASS /new: fresh Session with its own model context', flush=True)
        # /resume back to the first Session: history visible, context continues.
        m = len(t.raw)
        t.send('/resume\r')
        t.expect('Resume session', m)
        t.expect('(current)', m)
        t.send('\x1b[B\r')  # the other (older) Session
        t.expect('alpha second', m)  # its transcript is shown again
        m = len(t.raw)
        t.send('alpha third\r')
        t.expect('turns-seen-3-alpha_third', m)
        print('PASS /resume: earlier Session restored with transcript and model context', flush=True)
    finally:
        t.close('sessions')
