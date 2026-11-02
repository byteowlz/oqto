#!/usr/bin/env python3
"""Startup speed mechanisms: the host lingers after the last client leaves
(OQT_HOST_IDLE_SECONDS) so the next launch attaches without starting a host,
and the runtime is not pinned to one CPU at nice 19."""
import json
import os
import subprocess
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world
from tui_test import Terminal

os.environ['OQT_HOST_IDLE_SECONDS'] = '60'
with coding_world(lambda request: ('text', 'warm-reply'), 'oqt-startup-') as (provider, owner, work):
    source = os.environ.get('OQT_PI_SOURCE', str(Path.home()/'.local/share/oqt/pi-source'))
    prepared = json.loads(subprocess.check_output([
        'python3', str(Path(__file__).resolve().parent/'prepare.py'), source,
        str(work), 'coding', str(owner)
    ], text=True))
    manifest = Path(prepared['profile'])/'host.json'
    previous_host = None
    timings = []
    for launch in ('cold', 'warm'):
        started = time.monotonic()
        t = Terminal(['--owner-profile', str(owner), '--workdir', str(work)])
        t.deadline = time.monotonic() + 90
        try:
            t.expect('oqt coding')
            timings.append(round(time.monotonic() - started, 2))
            current_host = json.loads(manifest.read_text())['pid']
            if launch == 'warm':
                assert current_host == previous_host, 'Warm launch replaced the live host'
            previous_host = current_host
            if launch == 'warm':
                pid = int(os.popen(f"pgrep -f 'oqt-launcher.ts' -n").read().strip() or 0)
                if pid:
                    nice = int(open(f'/proc/{pid}/stat').read().rsplit(')', 1)[1].split()[16])
                    assert nice == 0, f'runtime runs at nice {nice}'
                    allowed = open(f'/proc/{pid}/status').read().split('Cpus_allowed_list:')[1].split()[0]
                    assert allowed == open('/proc/self/status').read().split('Cpus_allowed_list:')[1].split()[0], allowed
            m = len(t.raw)
            t.send('hello\r')
            t.expect('warm-reply', m)
        finally:
            t.close(f'startup-{launch}')
    print(f'PASS startup: warm launch reuses the lingering host (cold {timings[0]}s, warm {timings[1]}s); '
          'runtime at normal priority on all CPUs', flush=True)
