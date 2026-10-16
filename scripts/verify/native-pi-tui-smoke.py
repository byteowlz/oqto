"""Opt-in real Pi terminal fixture; run with uv, never an operator profile.

No prompt, inference, input lease or service is used. The fresh Pi owns its
session file. Only the exact process launched here is terminated/reaped.
"""

import argparse
import errno
import fcntl
import hashlib
import http.server
import json
import os
import pty
import select
import shutil
import struct
import subprocess
import tempfile
import termios
import threading
import time
import uuid
from pathlib import Path


class OfflineTrap(http.server.BaseHTTPRequestHandler):
    requests = 0

    def do_POST(self):
        type(self).requests += 1
        self.send_error(503, "Fixture forbids inference")

    def do_GET(self):
        type(self).requests += 1
        self.send_error(503, "Fixture forbids network requests")

    def log_message(self, *_args):
        return


def prepare(root, extension, trap_port):
    for name in [
        "home",
        "profile",
        "sessions",
        "sockets",
        "work",
        "config",
        "cache",
        "data",
    ]:
        (root / name).mkdir(mode=0o700)
    provider = {
        "baseUrl": f"http://127.0.0.1:{trap_port}/v1",
        "api": "openai-completions",
        "apiKey": "synthetic-not-a-credential",
        "models": [
            {
                "id": "no-inference",
                "name": "Offline transport fixture",
                "reasoning": False,
                "input": ["text"],
                "contextWindow": 8192,
                "maxTokens": 128,
                "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
            }
        ],
    }
    (root / "profile/models.json").write_text(
        json.dumps({"providers": {"fixture": provider}})
    )
    (root / "profile/settings.json").write_text(
        json.dumps(
            {
                "defaultProvider": "fixture",
                "defaultModel": "no-inference",
                "quietStartup": True,
                "compaction": {"enabled": False},
            }
        )
    )
    # Allowlist instead of copying an environment full of provider credentials.
    env = {
        "PATH": os.environ["PATH"],
        "HOME": str(root / "home"),
        "TERM": "xterm-256color",
        "LANG": "en_US.UTF-8",
        "SHELL": "/bin/sh",
        "TMPDIR": str(root / "sockets"),
        "XDG_CONFIG_HOME": str(root / "config"),
        "XDG_CACHE_HOME": str(root / "cache"),
        "XDG_DATA_HOME": str(root / "data"),
        "PI_CODING_AGENT_DIR": str(root / "profile"),
        "PI_OFFLINE": "1",
        "PI_TELEMETRY": "0",
        "PI_TUI_RPC_LEASE_CONFIRM": "1",
    }
    args = [
        "--offline",
        "--no-tools",
        "--no-mcp",
        "--no-extensions",
        "--no-skills",
        "--no-prompt-templates",
        "--no-themes",
        "--no-context-files",
        "--no-approve",
        "--session",
        str(root / "sessions/disposable.jsonl"),
        "--name",
        "Oqto Native Observation Fixture",
        "--extension",
        str(extension),
    ]
    # Pi lazily creates an empty new session's JSONL. Seed an explicitly
    # synthetic history before launch, then let Pi alone own it thereafter.
    seed_session(root)
    return env, args


def seed_session(root):
    stamp = "2026-01-01T00:00:00.000Z"
    entries = [
        {
            "type": "session",
            "version": 3,
            "id": str(uuid.uuid4()),
            "timestamp": stamp,
            "cwd": str(root / "work"),
        },
        {
            "type": "message",
            "id": "fixture-user",
            "parentId": None,
            "timestamp": stamp,
            "message": {
                "role": "user",
                "content": [{"type": "text", "text": "Synthetic fixture."}],
                "timestamp": 1767225600000,
            },
        },
        {
            "type": "message",
            "id": "fixture-assistant",
            "parentId": "fixture-user",
            "timestamp": stamp,
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "No inference performed."}],
                "api": "openai-completions",
                "provider": "fixture",
                "model": "no-inference",
                "usage": {
                    "input": 0,
                    "output": 0,
                    "cacheRead": 0,
                    "cacheWrite": 0,
                    "totalTokens": 0,
                    "cost": {
                        "input": 0,
                        "output": 0,
                        "cacheRead": 0,
                        "cacheWrite": 0,
                        "total": 0,
                    },
                },
                "stopReason": "stop",
                "timestamp": 1767225600000,
            },
        },
    ]
    with (root / "sessions/disposable.jsonl").open("x") as source:
        source.write("".join(json.dumps(entry) + "\n" for entry in entries))


def drain_terminal(master, stop, log):
    written = 0
    with log.open("wb") as output:
        while not stop.is_set():
            readable, _, _ = select.select([master], [], [], 0.1)
            if not readable:
                continue
            try:
                data = os.read(master, 16384)
            except OSError as error:
                if error.errno == errno.EIO:
                    return  # Darwin/Linux PTY hangup after our child exits.
                raise
            if not data:
                return
            bounded = data[: max(0, 262144 - written)]
            output.write(bounded)
            output.flush()
            written += len(bounded)


def wait_for_session(root, child):
    session = root / "sessions/disposable.jsonl"
    socket = root / "sockets" / f"pi-tui-rpc-{child.pid}.sock"
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise RuntimeError(
                "Fixture Pi exited during startup; inspect private terminal.log"
            )
        if session.exists() and socket.exists():
            with session.open() as source:
                header = json.loads(source.readline())
            if header.get("type") != "session" or header.get("cwd") != str(
                root / "work"
            ):
                raise RuntimeError(
                    "Fixture session header is not the intended work directory"
                )
            return {
                "kind": "oqto-native-observation-v1",
                "external_id": header["id"],
                "session_file": str(session.resolve()),
                "pid": child.pid,
            }
        time.sleep(0.05)
    raise RuntimeError(
        "Fixture did not publish its session/socket; inspect private terminal.log"
    )


def run_probe(root, child, repo):
    manifest = wait_for_session(root, child)
    (root / "fixture.json").write_text(json.dumps(manifest))
    before = hashlib.sha256(
        (root / "sessions/disposable.jsonl").read_bytes()
    ).hexdigest()
    env = dict(os.environ, OQTO_NATIVE_TUI_FIXTURE=str(root))
    command = [
        "cargo",
        "test",
        "--locked",
        "--offline",
        "-p",
        "oqto-runner",
        "--lib",
        "tui_rpc::tests::native::disposable_real_tui_observation_is_read_only_and_reconnect_is_explicit",
        "--",
        "--ignored",
        "--exact",
        "--nocapture",
    ]
    with (root / "observer.log").open("wb") as log:
        result = subprocess.run(
            command,
            cwd=repo / "backend",
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=180,
            check=False,
        )
    alive = child.poll() is None
    unchanged = (
        before
        == hashlib.sha256((root / "sessions/disposable.jsonl").read_bytes()).hexdigest()
    )
    if result.returncode != 0 or not alive or not unchanged or OfflineTrap.requests:
        raise RuntimeError(
            "Native observer proof failed; inspect private observer.log/result.json"
        )
    return {
        "observer_status": result.returncode,
        "terminal_alive_after_observation": alive,
        "pi_jsonl_unchanged": unchanged,
        "model_requests": OfflineTrap.requests,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--extension", type=Path, required=True)
    parser.add_argument("--pi", default=shutil.which("pi"))
    args = parser.parse_args()
    if not args.pi or not args.extension.is_file():
        parser.error("An installed Pi and explicit pi-tui-rpc extension are required")
    os.umask(0o077)
    # The supported socket layouts must also fit Darwin sockaddr_un.sun_path.
    root = Path(tempfile.mkdtemp(prefix="oqto-pi-smoke-", dir="/tmp")).resolve()
    trap = http.server.ThreadingHTTPServer(("127.0.0.1", 0), OfflineTrap)
    trap_thread = threading.Thread(target=trap.serve_forever, daemon=True)
    trap_thread.start()
    env, pi_args = prepare(root, args.extension.resolve(), trap.server_port)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 100, 0, 0))
    stop = threading.Event()
    result = {
        "status": "failed",
        "model_requests": 0,
        "extension_sha256": hashlib.sha256(args.extension.read_bytes()).hexdigest(),
    }
    child = None
    drainer = None
    try:
        child = subprocess.Popen(
            [args.pi, *pi_args],
            cwd=root / "work",
            env=env,
            stdin=slave,
            stdout=slave,
            stderr=slave,
            start_new_session=True,
        )
        os.close(slave)
        slave = None
        drainer = threading.Thread(
            target=drain_terminal,
            args=(master, stop, root / "terminal.log"),
            daemon=True,
        )
        drainer.start()
        result.update(run_probe(root, child, Path(__file__).resolve().parents[2]))
        result["status"] = "passed"
    finally:
        if child is not None:
            if child.poll() is None:
                child.terminate()  # Only our direct child, never terminal/socket-discovered PIDs.
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
            result["owned_process_reaped"] = child.poll() is not None
        stop.set()
        if drainer is not None:
            drainer.join(timeout=1)
        os.close(master)
        if slave is not None:
            os.close(slave)
        trap.shutdown()
        trap.server_close()
        result["model_requests"] = OfflineTrap.requests
        if OfflineTrap.requests:
            result["status"] = "failed"
        (root / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print(
            f"Native observation fixture: {result['status']}; private evidence: {root}"
        )
    if result["status"] != "passed":
        raise RuntimeError("Offline fixture invariants were not preserved")


if __name__ == "__main__":
    main()
