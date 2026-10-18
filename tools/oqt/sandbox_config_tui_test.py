#!/usr/bin/env python3
"""ADR-0060 end to end in the real oqt TUI: approvals come from the one sandbox
config. The owner's sandbox.toml [guard] pre-approves `echo` (no prompt); the
workspace's .oqto/sandbox.toml [guard] denies edits (no prompt, no effect)."""
import json
import os
from pathlib import Path
import re
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world, tool_results
from tui_test import Terminal

CSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)")


def script(request):
    done = len(tool_results(request))
    if done == 0:
        return ('tool', 'bash', {'command': 'echo owner-preapproved'})
    if done == 1:
        return ('tool', 'write', {'path': 'blocked.txt', 'content': 'x'})
    return ('text', 'config-done')


with coding_world(script, 'oqt-sandbox-config-') as (provider, owner, work):
    user_config = Path(os.environ['OQT_SANDBOX_CONFIG'])
    user_config.write_text('profile = "development"\n\n[guard]\nauto_shell = ["echo"]\n')
    (work / '.oqto').mkdir()
    (work / '.oqto' / 'sandbox.toml').write_text('[guard]\nedit = "deny"\n')
    t = Terminal(['--owner-profile', str(owner), '--workdir', str(work)], env=dict(os.environ))
    t.deadline = time.monotonic() + 90
    try:
        t.expect('oqt coding')
        m = len(t.raw)
        t.send('go\r')
        t.expect('config-done', m)
        screen = CSI.sub(b'', t.raw[m:]).decode('utf8', 'replace')
        assert 'Do you want to proceed?' not in screen, 'neither call may prompt'
        results = json.dumps(tool_results(provider.calls[-1]))
        assert 'owner-preapproved' in results, 'owner auto_shell ran without a prompt'
        assert not (work / 'blocked.txt').exists(), 'workspace edit = "deny" blocked the write'
        print('PASS sandbox config: owner sandbox.toml auto_shell pre-approves; workspace '
              '.oqto/sandbox.toml edit=deny blocks without a prompt', flush=True)
    finally:
        t.close('sandbox-config')
    # A workspace file that tries to widen fails the session closed.
    (work / '.oqto' / 'sandbox.toml').write_text('[guard]\nedit = "auto"\n')
    t = Terminal(['--owner-profile', str(owner), '--workdir', str(work)], env=dict(os.environ))
    t.deadline = time.monotonic() + 60
    try:
        t.expect('the session could not start')
        t.expect('would widen approvals')
        assert not (work / 'blocked.txt').exists()
        print('PASS sandbox config: a widening workspace guard fails closed with its reason shown',
              flush=True)
    finally:
        t.close('sandbox-config-widen')
