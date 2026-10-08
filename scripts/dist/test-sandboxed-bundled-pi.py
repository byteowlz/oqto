#!/usr/bin/env python3
"""Private, runner-owned sandbox qualification. No production homes or stores.

Usage: test-sandboxed-bundled-pi.py RUNNER_BINARY QUALIFIED_NODE_PI_RUNTIME
Stages a release-shaped runner/runtime, requests minimal against a synthetic
reasoning provider with minimal:null, and asserts both RPC state and wire effort.
Pi alone writes native JSONL; the runner alone owns canonical history.
"""
import http.server
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import uuid


def main():
    if len(sys.argv) != 3 or sys.platform != "linux" or os.geteuid() == 0:
        raise SystemExit(
            "Run unprivileged on Linux: SCRIPT RUNNER_BINARY QUALIFIED_NODE_PI_RUNTIME"
        )
    runner, runtime = map(lambda x: Path(x).resolve(strict=True), sys.argv[1:])
    # /tmp is intentionally masked inside the sandbox. Keep the entire private
    # release-shaped fixture outside it rather than changing sandbox policy.
    output = Path(
        os.environ.get(
            "OQTO_QUALIFICATION_DIR", str(Path.home() / ".cache/oqto/qualification")
        )
    )
    output.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="sandboxed-pi-", dir=output))
    root.chmod(0o700)
    immutable = root / "immutable"
    (immutable / "bin").mkdir(parents=True)
    (immutable / "pi-runtime").mkdir()
    subprocess.run(
        ["cp", "--reflink=auto", str(runner), str(immutable / "bin/oqto-runner")],
        check=True,
    )
    subprocess.run(
        ["cp", "--reflink=auto", "-a", str(runtime), str(immutable / "pi-runtime/pi")],
        check=True,
    )
    home = root / "home"
    agent = home / ".pi/agent"
    agent.mkdir(parents=True)
    work = home / "work"
    work.mkdir()
    native = agent / "sessions/canary.jsonl"
    native.parent.mkdir()
    control = root / "control"
    control.mkdir(mode=0o700)
    endpoint = control / "runner.sock"
    received = []

    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            effort = request.get("reasoning_effort")
            received.append({"model": request.get("model"), "reasoning_effort": effort})
            if effort != "low":
                payload = json.dumps(
                    {
                        "error": {
                            "type": "invalid_request_error",
                            "code": "unsupported_value",
                            "message": "minimal unsupported in fixture",
                        }
                    }
                ).encode()
                self.send_response(400)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            for delta, finish in [
                ({"role": "assistant", "content": "OK"}, None),
                ({}, "stop"),
            ]:
                chunk = {
                    "id": "fixture",
                    "object": "chat.completion.chunk",
                    "created": 1,
                    "model": "fixture-reasoner",
                    "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
                }
                self.wfile.write(("data: " + json.dumps(chunk) + "\n\n").encode())
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (agent / "models.json").write_text(
        json.dumps(
            {
                "providers": {
                    "fixture": {
                        "baseUrl": f"http://127.0.0.1:{server.server_port}/v1",
                        "apiKey": "private-synthetic-fixture",
                        "api": "openai-completions",
                        "models": [
                            {
                                "id": "fixture-reasoner",
                                "reasoning": True,
                                "input": ["text"],
                                "contextWindow": 8192,
                                "maxTokens": 512,
                                "thinkingLevelMap": {
                                    "off": "none",
                                    "minimal": None,
                                    "low": "low",
                                    "medium": "medium",
                                    "high": "high",
                                    "xhigh": "xhigh",
                                    "max": "max",
                                },
                            }
                        ],
                    }
                }
            }
        )
    )
    sandbox = root / "sandbox.toml"
    sandbox.write_text(
        'enabled = true\nprofile = "development"\nseccomp_mode = "off"\n'
    )
    env = dict(
        os.environ,
        HOME=str(home),
        XDG_DATA_HOME=str(home / ".local/share"),
        XDG_CONFIG_HOME=str(home / ".config"),
        XDG_STATE_HOME=str(home / ".local/state"),
        XDG_RUNTIME_DIR=str(control),
        PI_CODING_AGENT_DIR=str(agent),
        PI_OFFLINE="1",
        RUST_LOG="debug",
    )
    # The fixture never borrows provider or Oqto credentials from the operator.
    for key in list(env):
        if (
            key.startswith(("EAVS_", "OQTO_", "HERDR_", "AGENT_BROWSER_", "XLATCH_"))
            or key == "SSH_AUTH_SOCK"
            or key.endswith(("API_KEY", "API_TOKEN", "AUTH_TOKEN"))
        ):
            env.pop(key)
    log = (root / "runner.log").open("wb")
    process = subprocess.Popen(
        [
            str(immutable / "bin/oqto-runner"),
            "--socket",
            str(endpoint),
            "--sandbox-config",
            str(sandbox),
        ],
        env=env,
        stdout=log,
        stderr=log,
        start_new_session=True,
    )
    sid = "oqto-" + str(uuid.uuid4())
    stream = None
    replies = []

    def rpc(kind, **fields):
        nonlocal stream
        if stream is None:
            connection = socket.socket(socket.AF_UNIX)
            connection.settimeout(30)
            connection.connect(str(endpoint))
            stream = connection.makefile("rwb", buffering=0)
        stream.write((json.dumps({"type": kind, **fields}) + "\n").encode())
        while True:
            line = stream.readline()
            if not line:
                raise RuntimeError("Runner connection closed")
            response = json.loads(line)
            if response.get("type") == "pi_event":
                continue
            replies.append({"request": kind, "response": response})
            if response.get("type") == "error":
                raise RuntimeError(json.dumps(response))
            return response

    ok = False
    try:
        deadline = time.monotonic() + 15
        while not endpoint.exists():
            if process.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError(
                    "Runner did not become ready; see protected fixture log"
                )
            time.sleep(0.05)
        rpc(
            "pi_create_session",
            session_id=sid,
            config={
                "cwd": str(work),
                "provider": "fixture",
                "model": "fixture-reasoner",
                "session_file": str(native),
            },
        )
        changed = rpc("pi_set_thinking_level", session_id=sid, level="minimal")
        assert changed.get("level") == "low", changed
        state = rpc("pi_get_state", session_id=sid)["state"]
        assert state["thinkingLevel"] == "low", state
        assert state["model"]["thinkingLevelMap"]["minimal"] is None, state
        native_id = state["sessionId"]
        rpc("pi_prompt", session_id=sid, message="Reply only OK. Do not call tools.")
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            time.sleep(0.1)
            if (
                received
                and not rpc("pi_get_state", session_id=sid)["state"]["isStreaming"]
            ):
                break
        assert received and all(
            r["reasoning_effort"] == "low" for r in received
        ), received
        text = rpc("pi_get_last_assistant_text", session_id=sid)
        assert text.get("text") == "OK", text
        assert (
            native.is_file()
        ), "Pi history must be durable on the host, not namespace-only"
        rpc("pi_close_session", session_id=sid)
        rpc(
            "pi_create_session",
            session_id=sid,
            config={"cwd": str(work), "continue_session": str(native)},
        )
        restored = rpc("pi_get_state", session_id=sid)["state"]
        assert (
            restored["sessionId"] == native_id
        ), "Cold restore must preserve native identity"
        assert restored["thinkingLevel"] == "low", restored
        assert restored["model"]["thinkingLevelMap"]["minimal"] is None, restored
        assert (
            rpc("pi_set_thinking_level", session_id=sid, level="minimal")["level"]
            == "low"
        )
        rpc(
            "pi_prompt",
            session_id=sid,
            message="Reply only OK again. Do not call tools.",
        )
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            time.sleep(0.1)
            # This is a fixture HTTP invocation counter, never message reconciliation.
            if (
                len(received) == 2
                and not rpc("pi_get_state", session_id=sid)["state"]["isStreaming"]
            ):
                break
        assert len(received) == 2 and all(
            r["reasoning_effort"] == "low" for r in received
        ), received
        assert rpc("pi_get_last_assistant_text", session_id=sid).get("text") == "OK"
        ok = True
    finally:
        if stream is not None:
            try:
                rpc("pi_close_session", session_id=sid)
            except (OSError, RuntimeError):
                pass
            stream.close()
        try:
            os.killpg(process.pid, signal.SIGTERM)
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        except ProcessLookupError:
            pass
        server.shutdown()
        server.server_close()
        log.close()
        (root / "receipt.json").write_text(
            json.dumps(
                {
                    "ok": ok,
                    "platform_id": sid,
                    "requests": received,
                    "replies": replies,
                },
                indent=2,
            )
        )
        print(json.dumps({"ok": ok, "receipt": str(root / "receipt.json")}))


if __name__ == "__main__":
    main()
