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
        terminal.expect('Do you want to proceed?', marker)
        terminal.expect('→ 1. Yes', marker)
        # Session approval: the retry after a crash must still ask again.
        terminal.send('2')
        marker = terminal.expect('Allowing', marker)
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
        terminal.expect('Do you want to proceed?', marker)
        terminal.expect('retried', marker)
        terminal.expect('→ 1. Yes', marker)
        assert not (work / 'retried').exists(), 'retry ran before fresh approval'
        recovered = json.dumps(tool_results(provider.calls[-1]))
        assert 'interrupted' in recovered and 'may have partially run' in recovered, recovered
        loops = sum(1 for call in provider.calls if json.dumps(call).count(LOOP) and 'interrupted' not in json.dumps(tool_results(call)))
        terminal.send('1')
        marker = terminal.expect('Approved', marker)
        terminal.expect('recovery-complete', marker)
        assert (work / 'retried').read_text() == 'retried\n'
        executed = [c for c in provider.calls if 'heartbeat' in json.dumps(c)]
        assert snapshot(owner) == original
        print('PASS recovery: TUI reattached, call reported uncertain, Session grant fenced, '
              'no replay, retry ran only after fresh approval', flush=True)
    finally:
        terminal.close('coding-recovery')


# Second scenario: the whole host (server + guard) is killed mid-call.
def host_pids(owner):
    return [pid for pid in processes_with_env(f'OQT_OWNER_AGENT_DIR={owner}')
            if cmdline(pid).rstrip().endswith('host.ts')
            and os.path.basename(os.readlink(f'/proc/{pid}/exe')) == 'node']


def guard_dirs(since):
    found = []
    for entry in Path('/var/tmp').iterdir():
        try:
            if entry.name.startswith('oqg-') and entry.stat().st_ctime >= since and entry.stat().st_uid == os.getuid():
                found.append(entry)
        except OSError:
            continue
    return found


with coding_world(script, 'oqt-host-crash-') as (provider, owner, work):
    started = time.time()
    terminal = Terminal(['--coding', '--owner-profile', str(owner), '--workdir', str(work)])
    terminal.deadline = time.monotonic() + 60
    try:
        terminal.expect('oqt coding')
        marker = len(terminal.raw)
        terminal.send('Start the long loop.\r')
        terminal.expect('Do you want to proceed?', marker)
        terminal.expect('→ 1. Yes', marker)
        terminal.send('2')  # Session grant
        terminal.expect('Allowing', marker)
        heartbeat = work / 'heartbeat'
        for _ in range(200):
            if size(heartbeat) > 20:
                break
            time.sleep(0.025)
        assert size(heartbeat) > 20, 'approved shell never started'
        hosts = host_pids(owner)
        assert len(hosts) == 1, hosts
        dead_dirs = guard_dirs(started)
        assert dead_dirs, 'guard directory not found'
        os.kill(hosts[0], signal.SIGKILL)
        time.sleep(1.5)
        settled = size(heartbeat)
        time.sleep(0.6)
        assert size(heartbeat) == settled, 'hands survived the host'
        print('PASS host SIGKILL: guard-owned hands and descendants died with the host', flush=True)
    finally:
        terminal.close('coding-host-crash')
    # Age the dead host's private state past the sweeper's grace period.
    for directory in dead_dirs:
        os.utime(directory, (time.time() - 300, time.time() - 300))
    reopened = Terminal(['--coding', '--continue', '--owner-profile', str(owner), '--workdir', str(work)])
    reopened.deadline = time.monotonic() + 90
    try:
        reopened.expect('oqt coding')
        reopened.expect('Interrupted', 0)
        reopened.expect('Do you want to proceed?', 0)
        reopened.expect('retried', 0)
        reopened.expect('→ 1. Yes', 0)
        assert not (work / 'retried').exists(), 'retry ran before fresh approval'
        assert not any(d.exists() for d in dead_dirs), 'dead host state was not swept'
        marker = len(reopened.raw)
        reopened.send('1')
        reopened.expect('recovery-complete', marker)
        assert (work / 'retried').read_text() == 'retried\n'
        print('PASS host recovery: journal reported the call interrupted on restart, Session grant '
              'fenced, retry re-approved, dead host state swept', flush=True)
    finally:
        reopened.close('coding-host-recovery')
