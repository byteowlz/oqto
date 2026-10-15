"""Disposable owning profile + scripted OpenAI-compatible provider for oqt PTY proofs."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import tempfile
import threading


class ScriptedProvider:
    """`script(request)` returns ('tool', name, args) or ('text', value)."""

    def __init__(self, script):
        self.script = script
        self.calls = []
        provider = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                assert self.headers.get('Authorization') == 'Bearer fixture-owner-key'
                request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                provider.calls.append(request)
                step = provider.script(request)
                if step[0] == 'tool':
                    count = len(provider.calls)
                    delta = {'tool_calls': [{'index': 0, 'id': f'call_{count}', 'type': 'function',
                        'function': {'name': step[1], 'arguments': json.dumps(step[2])}}]}
                    finish = 'tool_calls'
                else:
                    delta, finish = {'content': step[1]}, 'stop'
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.send_header('Connection', 'close')
                self.end_headers()
                for data, reason in [(delta, None), ({}, finish)]:
                    chunk = {'id': 'fixture', 'object': 'chat.completion.chunk', 'created': 1, 'model': 'owner-model',
                        'choices': [{'index': 0, 'delta': data, 'finish_reason': reason}]}
                    self.wfile.write(('data: ' + json.dumps(chunk) + '\n\n').encode())
                    self.wfile.flush()
                self.wfile.write(b'data: [DONE]\n\n')
                self.wfile.flush()
                self.close_connection = True

        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    @property
    def port(self):
        return self.server.server_port

    def close(self):
        self.server.shutdown()
        self.server.server_close()


def last_user_text(request):
    return json.dumps(next(m for m in reversed(request['messages']) if m['role'] == 'user'))


def tool_results(request):
    return [m for m in request['messages'] if m['role'] == 'tool']


def write_owner(owner, port, extra_auth=None):
    owner.mkdir(mode=0o700)
    files = {
        'auth.json': {'fixture': {'type': 'api_key', 'key': 'fixture-owner-key'}, **(extra_auth or {})},
        'settings.json': {'defaultProvider': 'fixture', 'defaultModel': 'owner-model', 'defaultThinkingLevel': 'off'},
        'models.json': {'providers': {'fixture': {'api': 'openai-completions', 'baseUrl': f'http://127.0.0.1:{port}/v1',
            'models': [{'id': 'owner-model', 'name': 'Owner model', 'reasoning': False, 'input': ['text'],
                'cost': {'input': 0, 'output': 0, 'cacheRead': 0, 'cacheWrite': 0},
                'contextWindow': 8192, 'maxTokens': 1024}]}}},
    }
    for name, value in files.items():
        (owner / name).write_text(json.dumps(value))
        (owner / name).chmod(0o600)


def snapshot(owner):
    return {p.name: p.read_bytes() for p in owner.iterdir() if p.is_file()}


@contextmanager
def coding_world(script, prefix):
    with tempfile.TemporaryDirectory(prefix=prefix, dir='/var/tmp') as root:
        root = Path(root)
        provider = ScriptedProvider(script)
        try:
            owner = root / 'owner'
            write_owner(owner, provider.port)
            work = root / 'workspace'
            work.mkdir()
            # Hermetic: never read the developer's ~/.config/oqto/sandbox.toml.
            # Tests keep the old 1 s host linger unless they opt in.
            os.environ.setdefault('OQT_HOST_IDLE_SECONDS', '1')
            previous = os.environ.get('OQT_SANDBOX_CONFIG')
            os.environ['OQT_SANDBOX_CONFIG'] = str(root / 'sandbox.toml')
            try:
                yield provider, owner, work
            finally:
                if previous is None:
                    os.environ.pop('OQT_SANDBOX_CONFIG', None)
                else:
                    os.environ['OQT_SANDBOX_CONFIG'] = previous
        finally:
            provider.close()


def processes_with_env(needle):
    """PIDs whose initial environment contains `needle` (owned by this user)."""
    found = []
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if entry.stat().st_uid != os.getuid():
                continue
            if needle.encode() in (entry / 'environ').read_bytes().split(b'\0'):
                found.append(int(entry.name))
        except OSError:
            continue
    return found


def cmdline(pid):
    try:
        return Path(f'/proc/{pid}/cmdline').read_bytes().replace(b'\0', b' ').decode('utf8', 'replace')
    except OSError:
        return ''
