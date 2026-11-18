#!/usr/bin/env python3
"""Real oqt TUI (confined brain, guard, approvals) calling a named remote
machine over the hands link (ADR-0086), plus a code-mode fan-out cell.
Disposable owning profile and scripted provider; the machine comes from
OQT_HANDS_LINK_CONFIG/STATE (paired, delegated) and HL_Q_MACHINE /
HL_Q_REMOTE_OS. Not live hosted-provider proof."""
import json
import os
from pathlib import Path
import tempfile
import time
from coding_fixture import ScriptedProvider, tool_results, write_owner
from tui_test import Terminal

MACHINE = os.environ['HL_Q_MACHINE']
OS_NAME = os.environ['HL_Q_REMOTE_OS']
stamp = f'tui-{int(time.time())}'
cell = f'''
const r = await Promise.allSettled([
  tools.bash({{ command: "echo fan-remote; uname -s", machine: "{MACHINE}" }}),
  tools.bash({{ command: "echo fan-local" }}),
]);
return r.map((x) => x.status);'''
sequence = [
    ('write', {'path': f'{stamp}/remote.txt', 'content': 'tui-remote\n', 'machine': MACHINE}),
    ('bash', {'command': f'uname -s; cat {stamp}/remote.txt', 'machine': MACHINE}),
    ('code', {'code': cell}),
]


def script(request):
    done = len(tool_results(request))
    if done < len(sequence):
        return ('tool', *sequence[done])
    return ('text', 'remote-tui-complete')


provider = ScriptedProvider(script)
with tempfile.TemporaryDirectory(prefix='oqt-remote-tui-', dir='/var/tmp') as root:
    root = Path(root)
    owner = root / 'owner'
    write_owner(owner, provider.port)
    work = root / 'workspace'
    work.mkdir()
    terminal = Terminal(['--coding', '--owner-profile', str(owner), '--workdir', str(work)])
    terminal.deadline = time.monotonic() + 150
    try:
        terminal.expect('oqt coding')
        terminal.send('Run the remote proof.\r')
        marker = len(terminal.raw)
        # write, bash, then the two nested bash calls of the cell: four prompts.
        for _ in range(4):
            marker = terminal.expect('Do you want to proceed?', marker)
            terminal.send('1')
            marker = terminal.expect('Approved', marker)
        terminal.expect('remote-tui-complete', marker)
        results = [m for m in provider.calls[-1]['messages'] if m['role'] == 'tool']
        texts = [json.dumps(m.get('content')) for m in results]
        assert f'[machine {MACHINE} \\u00b7 cwd' in texts[0] or f'[machine {MACHINE} · cwd' in texts[0], texts[0]
        assert 'tui-remote' in texts[1] and OS_NAME in texts[1], texts[1]
        assert 'fulfilled' in texts[2] and 'rejected' not in texts[2], texts[2]
        assert f'bash on {MACHINE}: ok' in texts[2] and 'bash on local: ok' in texts[2], texts[2]
        assert not (work / stamp).exists(), 'remote effects reached the local workspace'
        # Owner limits and connection state in the TUI (ADR-0086 owner decision).
        marker = len(terminal.raw)
        terminal.send('/machines\r')
        terminal.expect(f'{MACHINE}: connected', marker)
        terminal.expect('lease 10000 ms, renew 3000 ms, 4 calls', marker)
        print(f'PASS remote TUI: approvals in the real TUI, write/bash on {MACHINE} ({OS_NAME}), results name machine and cwd, code-mode fan-out one receipt per call, /machines shows limits', flush=True)
        for text in texts:
            print('RESULT', text[:300], flush=True)
    finally:
        terminal.close('remote-tui')
        provider.close()
