#!/usr/bin/env python3
"""Disposable actual CLI/guard/worker/TUI import, retry, append and continuation."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from coding_fixture import coding_world, write_owner
from tui_test import Terminal

ROOT = Path(__file__).resolve().parents[2]

def encoded(rows):
    return ''.join(json.dumps(row, separators=(',', ':')) + '\n' for row in rows)

with coding_world(lambda request: ('text', f'classic-resume-reply-{len(provider.calls)}'), 'oqt-classic-live-') as (provider, owner, work):
    source = work.parent / 'source archive.jsonl'
    header = {'type': 'session', 'version': 3, 'id': '9267f005-48a0-4c42-920e-3277aa2c6070', 'cwd': '/remote/mac/source-workspace', 'timestamp': '2026-10-17T00:00:00Z'}
    def row(identity, parent, message):
        return {'type': 'message', 'id': identity, 'parentId': parent, 'timestamp': header['timestamp'], 'message': message}
    usage = {'input': 0, 'output': 0, 'cacheRead': 0, 'cacheWrite': 0, 'totalTokens': 0, 'cost': {'input': 0, 'output': 0, 'cacheRead': 0, 'cacheWrite': 0, 'total': 0}}
    rows = [header,
            row('old-user', None, {'role': 'user', 'content': 'historical-user-marker ' + 'x' * (80 * 1024), 'timestamp': 1}),
            row('old-tool', 'old-user', {'role': 'assistant', 'api': 'openai-completions', 'provider': 'fixture', 'model': 'historical-unapproved-model', 'usage': usage, 'stopReason': 'toolUse', 'timestamp': 2, 'content': [{'type': 'toolCall', 'id': 'historical-call', 'name': 'bash', 'arguments': {'command': 'touch must-not-replay'}}]}),
            row('alternate', 'old-user', {'role': 'user', 'content': 'alternate-visible-marker', 'timestamp': 2}),
            row('old-result', 'old-tool', {'role': 'toolResult', 'toolCallId': 'historical-call', 'toolName': 'bash', 'isError': False, 'timestamp': 3, 'content': [{'type': 'text', 'text': 'historical-result-marker'}]})]
    source.write_text(encoded(rows))
    source.chmod(0o400)
    before = source.read_bytes()
    discovery_source = owner / 'sessions' / 'fixture' / 'source.jsonl'
    discovery_source.parent.mkdir(parents=True)
    discovery_source.write_bytes(before)
    discovery_source.chmod(0o400)
    def imported(owner_profile=owner, reject=False):
        result = subprocess.run([str(ROOT / 'oqt'), '--import-classic', str(source), '--origin', 'fixture-mac', '--owner-profile', str(owner_profile), '--workdir', str(work), '--json'], cwd=work, env=dict(os.environ), capture_output=True, text=True, timeout=180)
        if reject:
            assert result.returncode != 0 and not result.stdout.strip(), result
            return result.stderr
        if result.returncode:
            raise AssertionError(f'import CLI failed {result.returncode}: {result.stdout}\n{result.stderr}')
        return json.loads(result.stdout)
    first = imported()
    assert first['status'] == 'imported', first
    assert first['sourceHash'] == hashlib.sha256(before).hexdigest()
    assert provider.calls == [], 'import must not call a model'
    assert not (work / 'must-not-replay').exists(), 'historical tool call must not execute'
    same = imported()
    assert same['status'] == 'unchanged' and same['sessionId'] == first['sessionId'], {'first': first, 'retry': same, 'workspace': str(work)}
    assert source.read_bytes() == before, 'source bytes must remain exact'
    alias = work.parent / 'owner-alias'
    alias.symlink_to(owner, target_is_directory=True)
    canonical_retry = imported(alias)
    assert canonical_retry['sessionId'] == first['sessionId'] and canonical_retry['status'] == 'unchanged'
    other_owner = work.parent / 'other-owner'
    write_owner(other_owner, provider.port)
    separate = imported(other_owner)
    assert separate['sessionId'] != first['sessionId'] and separate['status'] == 'imported', 'different owners must never share the workspace host/catalog'
    assert provider.calls == []
    print('PASS classic import: real headless CLI >64KiB, native bootstrap/publish, retry identity, source immutability and no replay/inference', flush=True)
    if os.environ.get('OQT_IMPORT_HEADLESS_ONLY') == '1':
        sys.exit(0)
    t = Terminal(['--continue', '--owner-profile', str(owner), '--workdir', str(work)], env=dict(os.environ))
    t.deadline = time.monotonic() + 90
    try:
        t.expect('oqt coding')
        t.expect('historical-result-marker')
        assert provider.calls == [], 'rendering imported history must not infer or replay tools'
        marker = len(t.raw)
        t.send('/resume --classic --origin fixture-mac\r')
        t.expect('Import classic source', marker)
        t.send('\x1b')
        time.sleep(0.3)
        assert provider.calls == [], 'discovery/cancel must not infer or activate an import'
        assert discovery_source.read_bytes() == before
        marker = len(t.raw)
        t.send('continue this history\r')
        t.expect('classic-resume-reply-1', marker)
        messages = json.dumps(provider.calls[-1]['messages'])
        assert 'historical-user-marker' in messages and 'historical-result-marker' in messages, 'actual provider context preserves imported history'
        assert 'historical-unapproved-model' not in str(provider.calls[-1].get('model')), 'execution model stays owner-selected'
        assert not (work / 'must-not-replay').exists()
        t.close('classic-before-reopen')
        t = None
        time.sleep(1.5)
        t = Terminal(['--continue', '--owner-profile', str(owner), '--workdir', str(work)], env=dict(os.environ))
        t.deadline = time.monotonic() + 90
        t.expect('oqt coding')
        marker = len(t.raw)
        t.send('one more question\r')
        t.expect('classic-resume-reply-2', marker)
        assert 'historical-user-marker' in json.dumps(provider.calls[-1]['messages'])
        print('PASS classic continuation: actual TUI/provider context, owner model, client reopen and no historical effect replay', flush=True)
        t.close('classic-before-append')
        t = None
        rows.append(row('source-update', 'old-result', {'role': 'user', 'content': 'source-appended-marker', 'timestamp': 4}))
        source.chmod(0o600)
        source.write_text(encoded(rows))
        source.chmod(0o400)
        appended_bytes = source.read_bytes()
        appended = imported()
        assert appended['sessionId'] != first['sessionId']
        assert appended['predecessorSessionId'] == first['sessionId'] and appended['mode'] == 'new_snapshot', appended
        assert appended['reason'] == 'source_append_destination_preserved'
        assert len(provider.calls) == 2 and source.read_bytes() == appended_bytes
        t = Terminal(['--continue', '--owner-profile', str(owner), '--workdir', str(work)], env=dict(os.environ))
        t.deadline = time.monotonic() + 90
        t.expect('oqt coding')
        marker = len(t.raw)
        t.send('answer appended snapshot\r')
        t.expect('classic-resume-reply-3', marker)
        messages = json.dumps(provider.calls[-1]['messages'])
        assert 'source-appended-marker' in messages and 'one more question' not in messages
        marker = len(t.raw)
        t.send(f'/resume --classic {json.dumps(str(source))} --origin fixture-mac --leaf alternate\r')
        t.expect('alternate-visible-marker', marker)
        assert len(provider.calls) == 3, 'explicit classic resume must not infer'
        marker = len(t.raw)
        t.send('continue alternate branch\r')
        t.expect('classic-resume-reply-4', marker)
        messages = json.dumps(provider.calls[-1]['messages'])
        assert 'alternate-visible-marker' in messages and 'source-appended-marker' not in messages and 'historical-result-marker' not in messages
        marker = len(t.raw)
        t.send(f"/resume {first['sessionId']}\r")
        t.expect('one more question', marker)
        marker = len(t.raw)
        t.send('verify original destination\r')
        t.expect('classic-resume-reply-5', marker)
        messages = json.dumps(provider.calls[-1]['messages'])
        assert 'one more question' in messages and 'source-appended-marker' not in messages
        source.chmod(0o600)
        source.write_bytes(appended_bytes.replace(b'historical-user-marker', b'rewritten-user-marker'))
        source.chmod(0o400)
        rewritten_bytes = source.read_bytes()
        error = imported(reject=True)
        assert any(word in error.lower() for word in ('rewrit', 'changed', 'divergence', 'mutat')), error
        assert source.read_bytes() == rewritten_bytes and len(provider.calls) == 5
        assert not (work / 'must-not-replay').exists()
        print('PASS classic append: immutable new snapshot, published predecessor, original advanced destination preserved, native /resume and rewrite refused', flush=True)
    finally:
        if t is not None:
            t.close('classic-import')
