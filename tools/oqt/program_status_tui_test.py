#!/usr/bin/env python3
"""Real oqt TUI emits OSC 7501 program status: working during a turn, blocked
(permission) while an approval waits, done after, clear on exit. Covers both
the terminal support query (the PTY answers it) and the PI_PROGRAM_STATUS=1
override. Disposable owner/provider; status carries no prompt or arguments."""
import base64
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import select
import time

from coding_fixture import coding_world, tool_results
from tui_test import CSI, Terminal

STATUS = re.compile(rb"\x1b\]7501;([^\x07\x1b]*)(?:\x07|\x1b\\)")
QUERY = b"\x1b]7501;?\x1b\\"


def script(request):
    if not tool_results(request):
        return ('tool', 'bash', {'command': 'echo status-secret-argument'})
    return ('text', 'status-turn-done')


def records(raw):
    found = []
    for match in STATUS.finditer(raw):
        body = match.group(1).decode()
        if body.startswith('?'):
            continue
        pairs = dict(pair.split('=', 1) for pair in body.split(':'))
        if 'msg' in pairs:
            pairs['msg'] = base64.b64decode(pairs['msg']).decode()
        found.append(pairs)
    return found


class AnsweringTerminal(Terminal):
    """Answers the OSC 7501 support query like a supporting terminal."""

    def expect(self, text, after=0):
        while time.monotonic() < self.deadline:
            if QUERY in self.raw and not getattr(self, 'answered', False):
                os.write(self.master, QUERY)
                self.answered = True
            clean = CSI.sub(b"", bytes(self.raw[after:])).decode("utf8", "replace")
            if text in clean:
                return len(self.raw)
            if self.process.poll() is not None:
                raise AssertionError(f"TUI exited {self.process.returncode}: {clean[-2000:]}")
            if select.select([self.master], [], [], .1)[0]:
                data = os.read(self.master, 65536)
                self.raw.extend(data)
                if b"\x1b[6n" in data:
                    os.write(self.master, b"\x1b[1;1R")
        raise AssertionError(f"deadline waiting for {text!r}")


def run(label, terminal_class, env):
    with coding_world(script, f'oqt-status-{label}-') as (provider, owner, work):
        t = terminal_class(['--owner-profile', str(owner), '--workdir', str(work)], env={**os.environ, **env})
        t.deadline = time.monotonic() + 90
        try:
            t.expect('oqt coding')
            mark = len(t.raw)
            t.send('run it\r')
            t.expect('Do you want to proceed?', mark)
            t.send('1')
            t.expect('status-turn-done', mark)
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline and not any(r.get('state') == 'done' for r in records(bytes(t.raw[mark:]))):
                if select.select([t.master], [], [], .1)[0]:
                    t.raw.extend(os.read(t.master, 65536))
            # Exit ourselves and read to EOF: the terminal clears the status on stop.
            t.send('\x03'); time.sleep(.15); t.send('\x03')
            end = time.monotonic() + 5
            while time.monotonic() < end and t.process.poll() is None:
                if select.select([t.master], [], [], .1)[0]:
                    try:
                        t.raw.extend(os.read(t.master, 65536))
                    except OSError:
                        break
            while select.select([t.master], [], [], .1)[0]:
                try:
                    data = os.read(t.master, 65536)
                except OSError:
                    break
                if not data:
                    break
                t.raw.extend(data)
        finally:
            t.close(f'program-status-{label}')
        seen = records(bytes(t.raw))
        states = [r['state'] for r in seen]
        assert all(r.get('app') == 'oqt' for r in seen if r['state'] != 'clear'), seen
        assert 'working' in states, states
        blocked = [r for r in seen if r['state'] == 'blocked']
        assert blocked and blocked[0].get('kind') == 'permission', seen
        assert 'done' in states[states.index('working'):], states
        assert states[-1] == 'clear', f'status not cleared on exit: {states}'
        assert 'status-secret-argument' not in repr(seen) and 'run it' not in repr(seen), 'status leaked prompt/arguments'
        print(f'PASS program status ({label}): {" -> ".join(dict.fromkeys(states))}', flush=True)


run('query', AnsweringTerminal, {})
run('forced', Terminal, {'PI_PROGRAM_STATUS': '1'})
