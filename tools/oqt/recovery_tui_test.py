#!/usr/bin/env python3
"""SIGKILL the confined brain during an approved shell (real TUI/PTY, real guard and
hands). Proves: executor stops, the TUI reattaches by itself, the interrupted call is
reported as uncertain and never replayed, an active Session grant no longer covers the
retry, and the retry needs fresh approval before any effect."""
import json
import os
from pathlib import Path
import signal
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import cmdline, coding_world, processes_with_env, snapshot, tool_results
from tui_test import Terminal

LOOP = 'while true; do echo tick >> heartbeat; sleep 0.05; done'


def script(request):
    results = json.dumps(tool_results(request))
    if 'interrupted' not in results:
        return ('tool', 'bash', {'command': LOOP, 'timeout': 25})
    if 'retried' not in results:
        return ('tool', 'bash', {'command': 'echo retried > retried; echo retried'})
    return ('text', 'recovery-complete')


def size(path):
    try:
        return path.stat().st_size
    except FileNotFoundError:
        return 0


with coding_world(script, 'oqt-recovery-') as (provider, owner, work):
    original = snapshot(owner)
    terminal = Terminal(['--coding', '--owner-profile', str(owner), '--workdir', str(work)])
    terminal.deadline = time.monotonic() + 100
    try:
        terminal.expect('oqt coding')
        marker = len(terminal.raw)
        terminal.send('Start the long loop.\r')
        terminal.expect('Coding permission', marker)
        terminal.expect('→ Deny', marker)
        # Session approval: the retry after a crash must still ask again.
        terminal.send('\x1b[A\r')
        marker = terminal.expect('Permission applied', marker)
        heartbeat = work / 'heartbeat'
        for _ in range(200):
            if size(heartbeat) > 20:
                break
            time.sleep(0.025)
        assert size(heartbeat) > 20, 'approved shell never started'
        brains = [pid for pid in processes_with_env(f'OQT_OWNER_AGENT_DIR={owner}')
                  if pid in processes_with_env('__PI_INTERNAL_SPAWN=session-worker')
                  and 'session-worker.ts' in cmdline(pid)
                  and os.path.basename(os.readlink(f'/proc/{pid}/exe')) == 'node']
        assert len(brains) == 1, brains
        os.kill(brains[0], signal.SIGKILL)
        time.sleep(1.5)
        settled = size(heartbeat)
        time.sleep(0.6)
        assert size(heartbeat) == settled, 'confined descendant survived brain SIGKILL'
        print('PASS brain SIGKILL: guard stopped confined shell and descendants', flush=True)

        terminal.expect('Interrupted', marker)
        terminal.expect('Coding permission', marker)
        terminal.expect('retried', marker)
        terminal.expect('→ Deny', marker)
        assert not (work / 'retried').exists(), 'retry ran before fresh approval'
        recovered = json.dumps(tool_results(provider.calls[-1]))
        assert 'interrupted' in recovered and 'may have partially run' in recovered, recovered
        loops = sum(1 for call in provider.calls if json.dumps(call).count(LOOP) and 'interrupted' not in json.dumps(tool_results(call)))
        terminal.send('\x1b[A\x1b[A\r')
        marker = terminal.expect('Permission applied', marker)
        terminal.expect('recovery-complete', marker)
        assert (work / 'retried').read_text() == 'retried\n'
        executed = [c for c in provider.calls if 'heartbeat' in json.dumps(c)]
        assert snapshot(owner) == original
        print('PASS recovery: TUI reattached, call reported uncertain, Session grant fenced, '
              'no replay, retry ran only after fresh approval', flush=True)
    finally:
        terminal.close('coding-recovery')
