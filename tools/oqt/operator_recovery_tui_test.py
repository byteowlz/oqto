#!/usr/bin/env python3
"""Kill only hosts positively bound to this disposable work directory.
Real operator execution/descendants, protected journal and native SQLite reopen;
no provider calls or ordinary profiles. pidfds prevent PID-reuse retargeting.
"""
import json
import os
from pathlib import Path
import signal
import stat
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world
from profile_namespace import workspace_profile_key
from tui_test import Terminal


def kill_fixture_host(work, owner):
    key = workspace_profile_key(work, 'coding', owner)
    profiles = []
    for candidate in Path('/var/tmp').glob(f'q{os.getuid()}-*/p/{key}'):
        metadata = candidate.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
            continue
        binding = json.loads((candidate / 'work-directory.json').read_text())
        if binding == {'work_directory': str(work.resolve())} and (candidate / 'host.json').is_file():
            profiles.append(candidate)
    assert len(profiles) == 1, 'cannot uniquely identify this fixture host'
    profile = profiles[0]
    host = json.loads((profile / 'host.json').read_text())
    descriptor = os.pidfd_open(host['pid'])
    try:
        command = Path(f"/proc/{host['pid']}/cmdline").read_bytes().split(b'\0')
        expected = str(profile.parents[1] / 'pi-source/packages/coding-agent/src/experimental/host.ts').encode()
        assert expected in command, 'fixture host executable binding changed'
        environment = Path(f"/proc/{host['pid']}/environ").read_bytes().split(b'\0')
        assert os.fsencode(f'OQT_OWNER_AGENT_DIR={owner.resolve()}') in environment, 'fixture host owner binding changed'
        assert json.loads((profile / 'host.json').read_text())['pid'] == host['pid']
        signal.pidfd_send_signal(descriptor, signal.SIGKILL)
    finally:
        os.close(descriptor)


def terminal(owner, work, resume=False):
    args = ['--owner-profile', str(owner), '--workdir', str(work)]
    if resume:
        args.append('--continue')
    instance = Terminal(args, env=dict(os.environ))
    instance.deadline = time.monotonic() + 90
    try:
        instance.expect('oqt coding')
        return instance
    except BaseException:
        instance.close('operator-startup-failed')
        raise


with coding_world(lambda request: ('text', 'must-not-be-called'), 'oqt-operator-crash-') as (provider, owner, work):
    current = terminal(owner, work)
    try:
        current.send('!printf once >> attempts; while true; do echo tick >> heartbeat; sleep 0.05; done\r')
        deadline = time.monotonic() + 10
        while not (work / 'heartbeat').exists() and time.monotonic() < deadline:
            time.sleep(0.025)
        assert (work / 'heartbeat').exists(), 'operator effect never started'
        kill_fixture_host(work, owner)
        time.sleep(1)
        settled = (work / 'heartbeat').stat().st_size
        time.sleep(0.6)
        assert (work / 'heartbeat').stat().st_size == settled, 'operator descendants survived host death'
        current.close('operator-killed-active')
        current = None
        current = terminal(owner, work, resume=True)
        current.expect('Interrupted')
        assert (work / 'attempts').read_text() == 'once', 'interrupted operator effect was replayed'
        assert provider.calls == [], 'recovery started inference'
        print('PASS operator host SIGKILL: descendants stopped, opaque invocation recovered, no replay/inference', flush=True)

        marker = len(current.raw)
        current.send('!printf once >> completed-attempts; printf completed-durable-value\r')
        current.expect('durably recorded for this Session', marker)
        kill_fixture_host(work, owner)
        current.close('operator-killed-completed')
        current = None
        current = terminal(owner, work, resume=True)
        marker = len(current.raw)
        current.send('/shell-results\r')
        current.expect('completed-durable-value', marker)
        assert (work / 'completed-attempts').read_text() == 'once', 'completed effect was replayed'
        assert (work / 'attempts').read_text() == 'once', 'older uncertain effect was replayed'
        assert provider.calls == [], 'completed-result recovery started inference'
        print('PASS completed-result SIGKILL: native receipt and sealed acknowledgment survive process death', flush=True)
    finally:
        if current is not None:
            current.close('operator-final-recovery')
