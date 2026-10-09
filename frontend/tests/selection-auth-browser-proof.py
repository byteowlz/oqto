#!/usr/bin/env python3
"""Opt-in acceptance against a disposable REAL authenticated gateway.
Never starts/stops an existing service; never mocks RPC, Status, or records.
Tokens must be synthetic fixture credentials (not production credentials).
"""
import argparse
import json
from pathlib import Path
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from threading import Thread
import atexit
from urllib.parse import urlencode
from playwright.sync_api import sync_playwright

p = argparse.ArgumentParser()
origin = p.add_mutually_exclusive_group(required=True)
origin.add_argument('--url', help='Already running disposable fixture HTTP origin')
origin.add_argument('--build-dir', help='Serve built fixture on private ephemeral loopback HTTP port')
p.add_argument('--gateway', required=True, help='Production control-plane origin; live adapter appends /api/ws/mux')
p.add_argument('--session', required=True)
p.add_argument('--token', required=True)
p.add_argument('--foreign-session', required=True)
p.add_argument('--foreign-token', required=True)
p.add_argument('--output', required=True)
a = p.parse_args()
out = Path(a.output)
out.mkdir(parents=True, exist_ok=True)
if a.build_dir:
    class QuietHandler(SimpleHTTPRequestHandler):
        def log_message(self, *_args):
            pass  # Never log synthetic credential-bearing query strings.
    server = ThreadingHTTPServer(('127.0.0.1', 0), partial(QuietHandler, directory=a.build_dir))
    Thread(target=server.serve_forever, daemon=True).start()
    atexit.register(server.server_close)
    atexit.register(server.shutdown)
    a.url = f'http://127.0.0.1:{server.server_port}'

def route(token):
    return a.url + '/?' + urlencode(dict(gateway=a.gateway, token=token, session=a.session))

def request(page, op, args, session=None):
    response = page.evaluate('([op,args,route]) => window.selectionProof.request(op,args,route || undefined)', [op, args, session])
    return response

def success(response):
    assert response.get('success') is True, response
    reply = response['data']['reply']
    assert reply.get('ok') is True, response
    result = reply.get('result', reply)
    return result.get('record', result)

def record(page):
    return page.evaluate('window.selectionProof.snapshot().record')

def wait_ready(page):
    try:
        page.wait_for_function('window.selectionProof && window.selectionProof.snapshot().phase === "ready"', timeout=15000)
    except Exception:
        page.screenshot(path=str(out / 'blocked.png'), full_page=True)
        (out / 'blocked.json').write_text(json.dumps(page.evaluate('({snapshot: window.selectionProof?.snapshot(), status: window.selectionProof?.status(), outbox: window.selectionProof?.outbox()})'), indent=2))
        raise

def wait_idle(page):
    page.wait_for_function('!window.selectionProof.snapshot().busy')

with sync_playwright() as pw:
    browser = pw.chromium.launch(headless=True)
    context = browser.new_context()
    page = context.new_page()
    errors, sent, received = [], [], []
    page.on('pageerror', lambda e: errors.append(str(e)))
    def websocket(ws):
        ws.on('framesent', lambda frame: sent.append(json.loads(frame)))
        ws.on('framereceived', lambda frame: received.append(json.loads(frame)))
    page.on('websocket', websocket)
    page.goto(route(a.token))
    wait_ready(page)
    questions = {'version': 1, 'mode': 'questions', 'title': 'Authenticated Questions', 'questions': [
        {'id': 'direction', 'header': 'Direction', 'title': 'Direction', 'kind': 'single', 'required': True, 'options': [{'id': 'small', 'label': 'Small'}, {'id': 'full', 'label': 'Full'}]},
        {'id': 'summary', 'header': 'Summary', 'title': 'Summary', 'kind': 'text', 'multiline': True, 'required': True}]}
    success(request(page, 'selection_create', {'presentation': 'host', 'spec': questions}))
    page.get_by_role('heading', name='Authenticated Questions').wait_for()
    page.get_by_role('radio', name='Small', exact=True).check()
    page.get_by_role('textbox', name='Answer', exact=True).fill('Durable authenticated answer')
    page.get_by_role('button', name='Save draft', exact=True).click()
    wait_idle(page)
    q = record(page)
    qid = q['id']
    durable = success(request(page, 'selection_get', {'id': qid}))
    assert durable['answers'] == q['answers'], durable
    page.screenshot(path=str(out / 'questions-saved.png'), full_page=True)
    page.reload()
    wait_ready(page)
    page.get_by_role('textbox', name='Answer', exact=True).wait_for()
    assert page.get_by_role('textbox', name='Answer', exact=True).input_value() == 'Durable authenticated answer'
    # Competing authorized control updates durable revision, NOT local edits.
    page.get_by_role('textbox', name='Answer', exact=True).fill('Unsaved local answer')
    fresh = record(page)
    success(request(page, 'selection_save', {'id': qid, 'revision': fresh['revision'], 'answers': fresh['answers']}))
    page.get_by_role('button', name='Save draft', exact=True).click()
    page.wait_for_function('!!window.selectionProof.snapshot().error')
    assert page.get_by_role('textbox', name='Answer', exact=True).input_value() == 'Unsaved local answer'
    assert page.evaluate('window.selectionProof.snapshot().dirty') is True
    page.screenshot(path=str(out / 'stale-cas-local-preserved.png'), full_page=True)
    page.get_by_role('button', name='Reload', exact=True).click()
    page.get_by_role('button', name='Confirm', exact=True).click()
    wait_idle(page)
    assert page.get_by_role('textbox', name='Answer', exact=True).input_value() == 'Durable authenticated answer'
    bindings = [event['data']['binding_id'] for event in received if event.get('cmd') == 'extension_control' and event.get('success') and event.get('data', {}).get('operation') == 'presentation_bind']
    page.evaluate('window.selectionProof.reconnect()')
    wait_ready(page)
    page.wait_for_function('window.selectionProof.snapshot().record !== null')
    page.wait_for_timeout(300)
    after = [event['data']['binding_id'] for event in received if event.get('cmd') == 'extension_control' and event.get('success') and event.get('data', {}).get('operation') == 'presentation_bind']
    assert bindings and after[-1] != bindings[-1], (bindings, after)
    page.get_by_role('button', name='Submit', exact=True).click()
    page.wait_for_function('window.selectionProof.snapshot().record.state === "submitted"')
    submitted = success(request(page, 'selection_get', {'id': qid}))
    assert submitted['state'] == 'submitted', submitted
    page.screenshot(path=str(out / 'questions-submitted.png'), full_page=True)
    review = {'version': 1, 'mode': 'review', 'title': 'Authenticated Review', 'items': [{'id': 'apple', 'label': 'Apple'}, {'id': 'pear', 'label': 'Pear'}], 'choices': [{'id': 'keep', 'label': 'Keep'}, {'id': 'drop', 'label': 'Drop'}]}
    success(request(page, 'selection_create', {'presentation': 'host', 'spec': review}))
    page.get_by_role('heading', name='Authenticated Review').wait_for()
    page.get_by_role('radio', name='Keep', exact=True).first.check()
    page.get_by_role('button', name='Save draft', exact=True).click()
    wait_idle(page)
    r = record(page)
    assert success(request(page, 'selection_get', {'id': r['id']}))['answers'] == r['answers']
    page.reload()
    wait_ready(page)
    page.get_by_role('heading', name='Authenticated Review').wait_for()
    assert page.get_by_role('radio', name='Keep', exact=True).first.is_checked()
    page.get_by_role('radio', name='Drop', exact=True).nth(1).check()
    page.get_by_role('button', name='Submit', exact=True).click()
    page.wait_for_function('window.selectionProof.snapshot().record.state === "submitted"')
    assert success(request(page, 'selection_get', {'id': r['id']}))['state'] == 'submitted'
    page.screenshot(path=str(out / 'review-submitted.png'), full_page=True)
    denied = request(page, 'selection_get', {'id': qid}, a.foreign_session)
    assert denied.get('success') is False, denied
    other_context = browser.new_context()
    other = other_context.new_page()
    other.goto(route(a.foreign_token))
    other.wait_for_function('window.selectionProof && window.selectionProof.status().connection === "connected"')
    foreign_denied = request(other, 'selection_get', {'id': qid})
    assert foreign_denied.get('success') is False, foreign_denied
    assert not errors, errors
    acknowledgements = [f for f in sent if f.get('operation') == 'selection_ack']
    assert {qid, r['id']} <= {f.get('arguments', {}).get('id') for f in acknowledgements}, acknowledgements
    controls = [f for f in received if f.get('cmd') == 'extension_control' and f.get('success')]
    for f in controls:
        data = f['data']
        assert data['request_id'] != data['reply']['requestId'], 'Native metadata and mux correlations must remain distinct'
        assert data['native_session_id'] != f['session_id'], 'Native identity must not become public route identity'
        assert any(command.get('id') == f.get('id') and command.get('session_id') == f['session_id'] for command in sent), f
    assert page.evaluate('window.selectionProof.outbox()') == []
    assert not [f for f in sent if f.get('cmd') in ['prompt', 'steer', 'follow_up']], sent
    assert not [f for f in received if f.get('event') in ['message_start', 'message_update', 'message_end', 'turn_start', 'turn_end']], received
    # Persist only synthetic control traces, never credential-bearing URLs.
    (out / 'result.json').write_text(json.dumps({'pass': True, 'questions_id': qid, 'review_id': r['id'], 'bindings': after, 'denied': denied, 'cross_account_denied': foreign_denied, 'sent': sent, 'received': received, 'errors': errors}, indent=2))
    other_context.close()
    context.close()
    browser.close()
print(str(out / 'result.json'))
