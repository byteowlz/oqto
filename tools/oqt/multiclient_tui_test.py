#!/usr/bin/env python3
"""Two real TUIs on one workspace: the second attaches to the same host/guard and
Session, sees the pending approval, decides it (single winner), both observe the
turn, and the Session survives the first client leaving."""
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import cmdline, coding_world, last_user_text, processes_with_env, tool_results
from tui_test import Terminal


def script(request):
    results = json.dumps(tool_results(request))
    if 'second prompt' in last_user_text(request):
        return ('text', 'second-client-complete')
    if 'shared' not in results:
        return ('tool', 'bash', {'command': 'echo shared >> shared.txt; echo shared'})
    return ('text', 'multi-complete')


def hosts(owner):
    return [pid for pid in processes_with_env(f'OQT_OWNER_AGENT_DIR={owner}')
            if cmdline(pid).rstrip().endswith('host.ts') and 'node' in cmdline(pid)]


with coding_world(script, 'oqt-multi-') as (provider, owner, work):
    first = Terminal(['--coding', '--owner-profile', str(owner), '--workdir', str(work)])
    first.deadline = time.monotonic() + 120
    second = None
    try:
        first.expect('oqt coding')
        marker_a = len(first.raw)
        first.send('Write the shared file.\r')
        first.expect('Do you want to proceed?', marker_a)
        assert len(hosts(owner)) == 1
        second = Terminal(['--coding', '--continue', '--owner-profile', str(owner), '--workdir', str(work)])
        second.deadline = time.monotonic() + 90
        second.expect('oqt coding')
        assert len(hosts(owner)) == 1, 'second client must reuse the host and guard'
        marker_b = len(second.raw)
        second.expect('Do you want to proceed?', 0)
        second.expect('→ 1. Yes', 0)
        second.send('1')  # approve once from the second client
        second.expect('Approved', marker_b)
        # The first client's still-open dialog is now stale: its decision loses.
        first.send('1')
        first.expect('Permission decision failed', marker_a)
        first.expect('multi-complete', marker_a)
        second.expect('multi-complete', marker_b)
        assert (work / 'shared.txt').read_text() == 'shared\n', 'effect ran exactly once'
        print('PASS multi-client: one host/guard, shared Session, single-winner approval, '
              'both clients observe the turn', flush=True)
        first.close('multi-first')
        first = None
        marker_b = len(second.raw)
        second.send('Now the second prompt.\r')
        second.expect('second-client-complete', marker_b)
        assert len(hosts(owner)) == 1
        print('PASS multi-client: Session survives the first client leaving', flush=True)
    finally:
        if first:
            first.close('multi-first')
        if second:
            second.close('multi-second')
    deadline = time.monotonic() + 15
    while hosts(owner) and time.monotonic() < deadline:
        time.sleep(0.25)
    assert not hosts(owner), 'host must retire after the last client'
    print('PASS multi-client: host retired after the last client', flush=True)
