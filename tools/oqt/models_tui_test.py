#!/usr/bin/env python3
"""Classic-pi model ergonomics in the real oqt TUI: scoped models first in
/model, fuzzy search, Ctrl+P cycling through the scope, and /scoped-models
saving to the owner's pi settings (backed up, other keys preserved)."""
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world
from tui_test import Terminal

MODELS = ['alpha-model', 'beta-model', 'gamma-model']

with coding_world(lambda request: ('text', f"reply-from-{request['model']}"), 'oqt-models-') as (provider, owner, work):
    config = json.loads((owner / 'models.json').read_text())
    template = config['providers']['fixture']['models'][0]
    config['providers']['fixture']['models'] = [dict(template, id=m, name=m.replace('-', ' ').title()) for m in MODELS]
    (owner / 'models.json').write_text(json.dumps(config))
    settings = json.loads((owner / 'settings.json').read_text())
    settings.update(defaultModel='alpha-model', enabledModels=['fixture/alpha-model', 'fixture/gamma-model'],
                    unknownFutureKey={'keep': True})
    (owner / 'settings.json').write_text(json.dumps(settings))
    t = Terminal(['--owner-profile', str(owner), '--workdir', str(work)])
    t.deadline = time.monotonic() + 120
    try:
        t.expect('oqt coding')
        # /model: scoped first, fuzzy search narrows, Enter selects.
        m = len(t.raw)
        t.send('/model\r')
        t.expect('★ = scoped', m)
        t.expect('★ Alpha Model', m)
        t.send('gamma')
        t.expect('search: gamma', m)
        m = len(t.raw)
        t.send('\r')
        t.expect('Model: fixture/gamma-model', m)
        # Ctrl+P cycles within the scope: gamma -> alpha (beta is not scoped).
        m = len(t.raw)
        t.send('\x10')
        t.expect('Model: fixture/alpha-model', m)
        m = len(t.raw)
        t.send('\x10')
        t.expect('Model: fixture/gamma-model', m)
        # The selected model is the one the next turn uses.
        m = len(t.raw)
        t.send('hello\r')
        t.expect('reply-from-gamma-model', m)
        print('PASS models: scoped-first /model, fuzzy search, Ctrl+P cycles the scope, turn uses it', flush=True)
        # /scoped-models: add beta and save into the owner's pi settings.
        m = len(t.raw)
        t.send('/scoped-models\r')
        t.expect('Scoped models (2)', m)
        t.send('beta')  # search, then toggle
        t.expect('search: beta', m)
        t.send('\r')
        t.expect('Scoped models (3)', m)
        t.send('save')
        t.expect('search: save', m)
        m = len(t.raw)
        t.send('\r')
        t.expect('Saved 3 scoped models', m)
        saved = json.loads((owner / 'settings.json').read_text())
        assert sorted(saved['enabledModels']) == ['fixture/alpha-model', 'fixture/beta-model', 'fixture/gamma-model'], saved
        assert saved['unknownFutureKey'] == {'keep': True}, 'unknown keys preserved'
        assert saved['defaultModel'] == 'alpha-model', 'other settings preserved'
        assert (owner / 'settings.json.oqt-backup').exists(), 'backup before first write'
        print('PASS scoped-models: saved to owner pi settings with backup; other keys preserved', flush=True)
    finally:
        t.close('models')
