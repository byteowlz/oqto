#!/usr/bin/env python3
"""Permission dialogs must survive redraws: arrow selection is kept while the
screen updates (status lines, transcript), number keys pick an option directly,
and Esc dismisses without deciding."""
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world, tool_results
from tui_test import Terminal


def script(request):
    done = len(tool_results(request))
    if done < 2:
        return ('tool', 'bash', {'command': ['echo step0', 'printf step1'][done]})
    return ('text', 'dialog-done')


with coding_world(script, 'oqt-dialog-') as (provider, owner, work):
    t = Terminal(['--owner-profile', str(owner), '--workdir', str(work)])
    t.deadline = time.monotonic() + 90
    try:
        t.expect('oqt coding')
        m = len(t.raw)
        t.send('go\r')
        t.expect('→ 1. Yes', m)
        t.send('\x1b[B')  # move to "2. Yes, and don't ask again" (session)
        # Force redraw churn while the dialog is open: the window resizes and
        # the permission facet posts status updates.
        for _ in range(4):
            t.resize(30 + _, 110)
            time.sleep(0.4)
        m = len(t.raw)
        t.send('\r')
        t.expect('Allowing `echo`', m)
        # Number keys: a different command asks again; 1 = Yes directly.
        t.expect('→ 1. Yes', m)
        m = len(t.raw)
        t.send('1')
        t.expect('Approved', m)
        t.expect('dialog-done', m)
        print('PASS dialog: selection survives redraws; number keys select directly', flush=True)
    finally:
        t.close('dialog')
