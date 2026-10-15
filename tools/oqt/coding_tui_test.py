#!/usr/bin/env python3
"""Real TUI + real provider transport, disposable owning profile; not live hosted-provider proof."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import tempfile
import threading
import time
from tui_test import Terminal

calls = []
sequence = [
    ('write', {'path':'proof.txt','content':'before\n'}),
    ('edit', {'path':'proof.txt','oldText':'before','newText':'after'}),
    ('read', {'path':'proof.txt'}),
    ('bash', {'command':'cat proof.txt > shell-proof.txt'}),
]
class Provider(BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_POST(self):
        assert self.headers.get('Authorization') == 'Bearer fixture-owner-key'
        request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        calls.append(request)
        count = sum(message['role']=='tool' for message in request['messages'])
        if count < len(sequence):
            name, args = sequence[count]
            delta = {'tool_calls':[{'index':0,'id':f'coding_{count}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}]}
            finish = 'tool_calls'
        else:
            last_user = next(m for m in reversed(request['messages']) if m['role']=='user')
            delta = {'content':'coding-reopen-complete' if 'Continue the completed' in json.dumps(last_user) else 'coding-proof-complete'}; finish = 'stop'
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.send_header('Connection','close'); self.end_headers()
        for data, reason in [(delta,None),({},finish)]:
            chunk = {'id':'fixture','object':'chat.completion.chunk','created':1,'model':'owner-model','choices':[{'index':0,'delta':data,'finish_reason':reason}]}
            self.wfile.write(('data: '+json.dumps(chunk)+'\n\n').encode()); self.wfile.flush()
        self.wfile.write(b'data: [DONE]\n\n'); self.wfile.flush()
        self.close_connection = True

with tempfile.TemporaryDirectory(prefix='oqt-coding-ui-',dir='/var/tmp') as root:
    root = Path(root); owner = root/'owner'; owner.mkdir(mode=0o700)
    work = root/'workspace'; work.mkdir()
    server = ThreadingHTTPServer(('127.0.0.1',0),Provider)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    files = {
        'auth.json': {'fixture':{'type':'api_key','key':'fixture-owner-key'}},
        'settings.json': {'defaultProvider':'fixture','defaultModel':'owner-model','defaultThinkingLevel':'off'},
        'models.json': {'providers':{'fixture':{'api':'openai-completions','baseUrl':f'http://127.0.0.1:{server.server_port}/v1','models':[{'id':'owner-model','name':'Owner model','reasoning':False,'input':['text'],'cost':{'input':0,'output':0,'cacheRead':0,'cacheWrite':0},'contextWindow':8192,'maxTokens':1024}]}}},
    }
    for name, value in files.items():
        (owner/name).write_text(json.dumps(value)); (owner/name).chmod(0o600)
    original = {p.name:p.read_bytes() for p in owner.iterdir()}
    terminal = Terminal(['--coding','--owner-profile',str(owner),'--workdir',str(work)])
    terminal.deadline = time.monotonic()+60
    try:
        terminal.expect('fixture/owner-model')
        terminal.expect('oqt coding')
        terminal.send('Perform the coding proof.\r')
        marker = len(terminal.raw)
        for name, _ in sequence:
            if name == 'read':
                continue  # reads inside the workspace never ask
            terminal.expect('Do you want to proceed?',marker)
            terminal.expect('→ 1. Yes',marker)
            # Effects cannot begin before explicit approval.
            if name == 'write': assert not (work/'proof.txt').exists()
            terminal.send('1')
            marker = terminal.expect('Approved',marker)
        terminal.expect('coding-proof-complete',marker)
        assert (work/'proof.txt').read_text() == 'after\n'
        assert (work/'shell-proof.txt').read_text() == 'after\n'
        assert len(calls)==5, len(calls)
        assert original == {p.name:p.read_bytes() for p in owner.iterdir()}, 'owning profile changed'
        print('PASS coding TUI: owning default/auth -> provider HTTP -> exact approvals -> real write/edit/read/bash -> same-turn completion; profile unchanged',flush=True)
    finally:
        terminal.close('coding-usable')
    reopened = Terminal(['--coding','--continue','--owner-profile',str(owner),'--workdir',str(work)])
    try:
        reopened.expect('oqt coding')
        marker = len(reopened.raw)
        reopened.send('Continue the completed coding proof.\r')
        reopened.expect('coding-reopen-complete',marker)
        assert len(calls)==6 and sum(m['role']=='tool' for m in calls[-1]['messages'])==4
        assert original == {p.name:p.read_bytes() for p in owner.iterdir()}
        print('PASS coding reopen: original native history/model context retained; no tool effects replayed',flush=True)
    finally:
        reopened.close('coding-reopen'); server.shutdown(); server.server_close()
