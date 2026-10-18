#!/usr/bin/env python3
"""The model sees pi's real system prompt: owner global AGENTS.md, the ancestor
chain to the workspace, tool guidelines, cwd and the oqt rules addendum."""
import json
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world

ROOT = Path(__file__).resolve().parents[2]

with coding_world(lambda request: ('text', 'oqt-coding-ready'), 'oqt-prompt-') as (provider, owner, work):
    (owner / 'AGENTS.md').write_text('GLOBAL-MARKER-5521 owner instructions')
    (work.parent / 'AGENTS.md').write_text('ANCESTOR-MARKER-7713 parent instructions')
    (work / 'AGENTS.md').write_text('PROJECT-MARKER-3390 project instructions')
    result = subprocess.run([str(ROOT / 'oqt'), '--smoke', '--owner-profile', str(owner), '--workdir', str(work)],
                            capture_output=True, text=True, timeout=200)
    assert result.returncode == 0, result.stderr[-2000:]
    system = json.dumps([m for m in provider.calls[0]['messages'] if m['role'] in ('system', 'developer')])
    for marker in ['GLOBAL-MARKER-5521', 'ANCESTOR-MARKER-7713', 'PROJECT-MARKER-3390',
                   'request_access', str(work), 'expert coding assistant']:
        assert marker in system, f'missing {marker!r} in system prompt'
    assert system.index('GLOBAL-MARKER-5521') < system.index('PROJECT-MARKER-3390'), 'global before project'
    print('PASS prompt: pi system prompt with owner + ancestor + project AGENTS.md, cwd and oqt rules', flush=True)
