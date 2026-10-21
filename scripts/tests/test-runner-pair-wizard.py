#!/usr/bin/env python3
"""Disposable wizard/CLI proofs. Never prints a QR/token or uses real profiles.

Default: exercise all wizard stages in a PTY, decline confirmation, verify no
state created. --bootstrap-lan explicitly opts into a short private-LAN TLS
fixture with synthetic HOME, empty workspaces, no inherited credentials and a
fixture-only process-census stub. It is NOT phone/tailnet or real-census proof.
"""

import argparse
import json
import os
import pathlib
import pty
import pwd
import re
import select
import signal
import socket
import subprocess
import sys
import tempfile
import time


def environment(home):
    return {
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_RUNTIME_DIR": str(home / "runtime"),
        "TMPDIR": str(home),
        "PATH": "/usr/bin:/bin",
        "USER": pwd.getpwuid(os.getuid()).pw_name,
        "LANG": "C.UTF-8",
        "TERM": "xterm-256color",
        "NO_COLOR": "1",
        "RUST_LOG": "warn",
    }


def wizard(binary, home):
    pid, fd = pty.fork()
    if pid == 0:
        os.execve(
            binary,
            [
                binary,
                "pair",
                "--interactive",
                "--workspace",
                str(home / "projects"),
                "--workspace",
                str(home / "research"),
            ],
            environment(home),
        )
    data, stage, reaped = b"", 0, False
    prompts = [b"Workspaces", b"Access", b"Connection", b"Enable pairing and show QR?"]
    try:
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            if select.select([fd], [], [], 0.1)[0]:
                try:
                    data += os.read(fd, 65536)
                except OSError:
                    break
            if stage < len(prompts) and prompts[stage] in data:
                os.write(fd, b"\r" if stage < 3 else b"n\r")
                stage += 1
            waited, _ = os.waitpid(pid, os.WNOHANG)
            if waited:
                reaped = True
                break
        assert stage == 4 and b"NOT restricted" in data and b"Pairing cancelled" in data
        assert not (home / ".local").exists(), "declined wizard changed state"
        assert b"projects" in data and b"research" in data
        return re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", data.decode(errors="replace"))
    finally:
        # Never signal a PID that has already been reaped and could be reused.
        if not reaped:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        os.close(fd)


def bootstrap(binary, home):
    assert sys.platform == "linux", "bootstrap fixture requires Linux pidfd cleanup"
    route = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    route.connect(("192.0.2.1", 9))  # Route selection only; no packet sent.
    ip = route.getsockname()[0]
    route.close()
    port_socket = socket.socket()
    port_socket.bind((ip, 0))
    port = port_socket.getsockname()[1]
    port_socket.close()
    (home / "runtime").mkdir(mode=0o700)
    (home / "bin").mkdir()
    probe = home / "bin/pgrep"
    probe.write_text("#!/bin/sh\nexit 1\n")
    probe.chmod(0o700)
    env = environment(home)
    env["PATH"] = str(home / "bin") + ":/usr/bin:/bin"
    cmd = [
        binary,
        "pair",
        "--workspace",
        str(home / "byteowlz"),
        "--workspace",
        str(home / "work"),
        "--access",
        "full",
        "--network",
        "lan",
        "--port",
        str(port),
        "--non-interactive",
        "--yes",
        "--output",
        "json",
    ]
    state = None
    try:
        first = subprocess.run(cmd, cwd=home, env=env, capture_output=True, timeout=30)
        status = subprocess.run(
            [binary, "pair", "--status"],
            cwd=home,
            env=env,
            capture_output=True,
            timeout=20,
        )
        if status.returncode == 0:
            state = json.loads(status.stdout)
        assert first.returncode == 0, (
            "private fixture pairing failed; diagnostics withheld"
        )
        qr1 = json.loads(first.stdout)
        assert qr1["purpose"] == "oqto.personal-host.v1" and len(qr1["token"]) == 64
        assert state and state["active"] and str(home) in state["state"]
        second = subprocess.run(cmd, cwd=home, env=env, capture_output=True, timeout=20)
        assert second.returncode == 0, "private fixture repeat pairing failed"
        qr2 = json.loads(second.stdout)
        after = json.loads(
            subprocess.run(
                [binary, "pair", "--status"],
                cwd=home,
                env=env,
                capture_output=True,
                check=True,
                timeout=20,
            ).stdout
        )
        assert state["pid"] == after["pid"]
        assert qr1["host_id"] == qr2["host_id"] and qr1["pin"] == qr2["pin"]
        assert qr1["token"] != qr2["token"]
        assert not (home / ".config/oqto/config.toml").exists()
        assert b"NOT restricted" in first.stderr
    finally:
        if state:
            pid = state["pid"]
            lease = os.pidfd_open(pid)
            try:
                cmdline = pathlib.Path(f"/proc/{pid}/cmdline").read_bytes()
                expected_runner = (
                    pathlib.Path(binary).with_name("oqto-runner").resolve()
                )
                assert str(home).encode() in cmdline
                assert (
                    pathlib.Path(os.fsdecode(cmdline.split(b"\0")[0])).resolve()
                    == expected_runner
                )
                signal.pidfd_send_signal(lease, signal.SIGTERM)
                ready, _, _ = select.select([lease], [], [], 8)
                assert ready, "owned fixture shutdown deadline"
            finally:
                os.close(lease)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument("--bootstrap-lan", action="store_true")
    parser.add_argument(
        "--snapshot",
        type=pathlib.Path,
        help="Create a new plain-text cancelled-wizard snapshot (no QR)",
    )
    args = parser.parse_args()
    binary = str(args.binary.resolve(strict=True))
    with tempfile.TemporaryDirectory(prefix="oqto-wizard-proof-") as raw:
        home = pathlib.Path(raw).resolve()
        for name in ["projects", "research", "byteowlz", "work"]:
            (home / name).mkdir()
        snapshot = wizard(binary, home)
        if args.snapshot:
            with args.snapshot.open("x") as output:
                output.write(snapshot)
        if args.bootstrap_lan:
            bootstrap(binary, home)
    print(
        json.dumps(
            {
                "ok": True,
                "pty_decline_without_mutation": True,
                "synthetic_bootstrap_lan": args.bootstrap_lan,
                "phone_tailnet_acceptance": False,
            }
        )
    )


if __name__ == "__main__":
    main()
