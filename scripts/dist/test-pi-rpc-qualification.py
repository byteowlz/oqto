#!/usr/bin/env python3
"""Qualify an explicit Pi binary using only loopback and disposable state.

This captures the real wire, not an SDK simulation. No user configuration,
credentials, model services, Oqto stores or live runtime links are touched.
It proves upstream behavior only; feed captures to Oqto's mirror/translator
regressions separately before promoting a runtime pin.
"""

import argparse
import json
import os
import queue
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class FixtureServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self):
        super().__init__(("127.0.0.1", 0), FixtureHandler)
        self.hold = False
        self.started = threading.Event()
        self.release = threading.Event()
        self.requests = 0


class FixtureHandler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        if self.path != "/v1/chat/completions" or not 0 < length < 1_048_576:
            self.send_error(400)
            return
        request = json.loads(self.rfile.read(length))
        if request.get("model") != "fixture":
            self.send_error(400)
            return
        self.server.requests += 1
        self.send_response(200)
        if not request.get("stream"):
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(
                json.dumps(
                    {
                        "id": "qualification",
                        "object": "chat.completion",
                        "created": 1,
                        "model": "fixture",
                        "choices": [
                            {
                                "index": 0,
                                "message": {
                                    "role": "assistant",
                                    "content": "Synthetic summary.",
                                },
                                "finish_reason": "stop",
                            }
                        ],
                    }
                ).encode()
            )
            return
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        try:
            for delta, reason in [
                ({"role": "assistant"}, None),
                ({"content": "Synthetic qualification answer.\u2028LF framing."}, None),
                ({}, "stop"),
            ]:
                chunk = {
                    "id": "qualification",
                    "object": "chat.completion.chunk",
                    "created": 1,
                    "model": "fixture",
                    "choices": [{"index": 0, "delta": delta, "finish_reason": reason}],
                }
                self.wfile.write(b"data: " + json.dumps(chunk).encode() + b"\n\n")
                self.wfile.flush()
                if "role" in delta and self.server.hold:
                    self.server.started.set()
                    if not self.server.release.wait(20):
                        return
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass  # Expected when the explicit abort case closes its stream.


class Rpc:
    def __init__(self, binary, home, cwd, stderr):
        env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_CACHE_HOME": str(home / ".cache"),
            "XDG_DATA_HOME": str(home / ".local/share"),
            "PI_CODING_AGENT_DIR": str(home / ".pi/agent"),
            "PI_OFFLINE": "1",
        }
        self.proc = subprocess.Popen(
            [
                str(binary),
                "--mode",
                "rpc",
                "-ne",
                "-ns",
                "-np",
                "-nc",
                "-na",
                "--offline",
                "--no-tools",
                "--provider",
                "oqto-qualification",
                "--model",
                "fixture",
                "--session-dir",
                str(home / "sessions"),
            ],
            cwd=cwd,
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=stderr,
        )
        self.incoming = queue.Queue()
        self.records = []
        self.counter = 0
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()

    def read(self):
        try:
            for line in self.proc.stdout:  # Binary LF framing preserves U+2028/U+2029.
                self.incoming.put(json.loads(line))
        except (OSError, UnicodeError, json.JSONDecodeError) as error:
            self.incoming.put(error)
        finally:
            self.incoming.put(EOFError("Pi stdout closed"))

    def send(self, kind, **fields):
        self.counter += 1
        request_id = f"qualification-{self.counter}"
        self.proc.stdin.write(
            json.dumps({"id": request_id, "type": kind, **fields}).encode() + b"\n"
        )
        self.proc.stdin.flush()
        return request_id

    def wait(self, predicate):
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            row = self.incoming.get(timeout=max(0.001, deadline - time.monotonic()))
            if isinstance(row, Exception):
                raise row
            self.records.append(row)
            if predicate(row):
                return row
        raise TimeoutError("Pi qualification deadline")

    def response(self, request_id):
        row = self.wait(
            lambda row: row.get("type") == "response" and row.get("id") == request_id
        )
        if row.get("success") is not True:
            raise AssertionError(f"RPC command failed: {row}")
        return row.get("data") or {}

    def command(self, kind, **fields):
        return self.response(self.send(kind, **fields))

    def settled(self):
        return self.wait(lambda row: row.get("type") == "agent_settled")

    def close(self):
        self.proc.stdin.close()
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=5)
        self.reader.join(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="New evidence directory; existing paths are rejected",
    )
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    version = subprocess.check_output(
        [str(binary), "--version"], text=True, timeout=10
    ).strip()
    if version != "1.0.0":
        raise SystemExit(f"Expected exact Pi 1.0.0, got {version}")
    args.output.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="oqto-pi-rpc-qualification-") as temp:
        root = Path(temp)
        home, cwd = root / "home", root / "project"
        agent_dir = home / ".pi/agent"
        agent_dir.mkdir(parents=True)
        cwd.mkdir()
        server = FixtureServer()
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        model = {
            "baseUrl": f"http://127.0.0.1:{server.server_port}/v1",
            "api": "openai-completions",
            "apiKey": "synthetic-no-secret",
            "models": [
                {
                    "id": "fixture",
                    "name": "Qualification fixture",
                    "contextWindow": 32768,
                    "maxTokens": 256,
                    "input": ["text"],
                    "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
                }
            ],
        }
        (agent_dir / "models.json").write_text(
            json.dumps({"providers": {"oqto-qualification": model}})
        )
        (agent_dir / "settings.json").write_text(
            json.dumps(
                {
                    "compaction": {"enabled": False, "keepRecentTokens": 1},
                    "retry": {
                        "baseDelayMs": 10,
                        "maxRetries": 1,
                        "provider": {"maxRetries": 0},
                    },
                }
            )
        )
        with (args.output / "stderr.log").open("wb") as stderr:
            rpc = Rpc(binary, home, cwd, stderr)
            try:
                rpc.command("set_auto_retry", enabled=False)
                rpc.command("set_auto_compaction", enabled=False)
                assert (
                    rpc.command("prompt", message="Synthetic plain turn.")[
                        "disposition"
                    ]
                    == "started"
                )
                rpc.settled()
                state = rpc.command("get_state")
                session_id, session_file = (
                    state["sessionId"],
                    Path(state["sessionFile"]),
                )
                assert not state["isStreaming"] and session_file.is_file()
                messages = rpc.command("get_messages")["messages"]
                assert any(
                    message["role"] == "system" and message.get("sections")
                    for message in messages
                )
                assert any(message["role"] == "assistant" for message in messages)
                plain = len(rpc.records)

                queue_requests_before = server.requests
                server.hold = True
                server.release.clear()
                assert (
                    rpc.command("prompt", message="Synthetic queued turn.")[
                        "disposition"
                    ]
                    == "started"
                )
                assert server.started.wait(10), (
                    "Mock provider did not receive streaming request"
                )
                assert (
                    rpc.command("follow_up", message="Synthetic follow-up.")[
                        "disposition"
                    ]
                    == "queued"
                )
                server.hold = False
                server.release.set()
                rpc.settled()
                queued = rpc.records[plain:]
                ends = [
                    index
                    for index, row in enumerate(queued)
                    if row["type"] == "agent_end"
                ]
                settles = [
                    index
                    for index, row in enumerate(queued)
                    if row["type"] == "agent_settled"
                ]
                assert ends and len(settles) == 1 and settles[0] > ends[-1], (
                    ends,
                    settles,
                )
                assert server.requests - queue_requests_before == 2, (
                    "Follow-up did not reach the provider"
                )
                after_queue = len(rpc.records)

                rpc.command("compact")
                compact_rows = rpc.records[after_queue:]
                assert any(row["type"] == "compaction_start" for row in compact_rows)
                assert any(row["type"] == "compaction_end" for row in compact_rows)
                after_compact = len(rpc.records)

                server.started.clear()
                server.release.clear()
                server.hold = True
                assert (
                    rpc.command("prompt", message="Synthetic abort turn.")[
                        "disposition"
                    ]
                    == "started"
                )
                assert server.started.wait(10)
                rpc.command(
                    "abort"
                )  # Response waits for idle, but consume any events before it.
                server.hold = False
                server.release.set()
                abort_rows = rpc.records[after_compact:]
                assert any(row["type"] == "agent_settled" for row in abort_rows)
                assert not rpc.command("get_state")["isStreaming"]

                entries = rpc.command("get_entries")["entries"]
                disk = [
                    json.loads(line)
                    for line in session_file.read_text().split("\n")
                    if line
                ]
                assert disk[0]["id"] == session_id and disk[0]["version"] == 3
                assert {entry["id"] for entry in entries} == {
                    entry["id"] for entry in disk[1:]
                }
                assert any(entry["type"] == "compaction" for entry in entries)
                # Only Pi writes this file. Capture it read-only for ingestion regression work.
                (args.output / "session.jsonl").write_bytes(session_file.read_bytes())
                (args.output / "events.jsonl").write_text(
                    "".join(
                        json.dumps(row, ensure_ascii=False) + "\n"
                        for row in rpc.records
                    )
                )
                (args.output / "report.json").write_text(
                    json.dumps(
                        {
                            "version": version,
                            "providerRequests": server.requests,
                            "wireRecords": len(rpc.records),
                            "sessionId": session_id,
                            "checks": [
                                "correlated responses",
                                "delta-only stream",
                                "LF Unicode framing",
                                "prompt disposition",
                                "queued follow-up settlement",
                                "manual compaction",
                                "abort settlement",
                                "system prompt transcript",
                                "native session identity",
                                "RPC/disk entry ID agreement",
                            ],
                            "limitations": [
                                "No live providers or user extensions",
                                "No Oqto translator/history acceptance",
                                "No tool execution, automatic retries, restart, fork or native Mac acceptance",
                            ],
                        },
                        indent=2,
                    )
                    + "\n"
                )
            finally:
                (args.output / "events.jsonl").write_text(
                    "".join(
                        json.dumps(row, ensure_ascii=False) + "\n"
                        for row in rpc.records
                    )
                )
                server.release.set()
                rpc.close()
                server.shutdown()
                server.server_close()
                worker.join(timeout=5)
    print(f"Pi {version} upstream RPC qualification passed; evidence: {args.output}")


if __name__ == "__main__":
    main()
