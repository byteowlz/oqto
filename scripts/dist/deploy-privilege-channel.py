#!/usr/bin/env python3
"""Deployment-scoped SSH privilege channel; passwords only traverse the terminal.

One sudo process owns the remote channel for its entire lifetime. This avoids
assuming tty/PPID-scoped sudo timestamps carry across independent SSH sessions.
The private local socket grants the invoking user the same deployment authority
as the interactive sudo session. No TCP listener, credential file or sudoers edit.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import select
import shlex
import signal
import socket
import subprocess
import sys
import termios
import time
import tty
import uuid

LIMIT = 8 * 1024 * 1024
# Only this fixed program is sent to sudo. Each subsequent trusted deployment
# script is framed JSON over the same authenticated channel, never a new sudo.
BROKER = r'''
import base64,json,os,signal,subprocess,sys,tempfile,tty
if os.geteuid() != 0: raise SystemExit("privileged channel is not root")
if os.isatty(0): tty.setraw(0)
signal.signal(signal.SIGHUP, signal.SIG_IGN)
print(json.dumps({"ready":sys.argv[1]}),flush=True)
for line in sys.stdin.buffer:
    request=json.loads(line)
    if request.get("op") == "close": break
    if request.get("op") != "run" or not isinstance(request.get("script"),str): break
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        # A connection failure must not interrupt an in-flight canonical root
        # transaction halfway through activation/rollback. Its process completes
        # independently; the channel admits no further commands after disconnect.
        child=subprocess.run(["bash","-lc",request["script"]],stdin=subprocess.DEVNULL,
                             stdout=out,stderr=err,start_new_session=True)
        result={"code":child.returncode}
        for name,stream in (("stdout",out),("stderr",err)):
            stream.seek(0)
            data=stream.read(8*1024*1024+1)
            if len(data)>8*1024*1024:
                data=data[:8*1024*1024]
                result["code"]=1
                result["truncated"]=True
            result[name]=base64.b64encode(data).decode("ascii")
    print(json.dumps(result),flush=True)
'''


def read_frame(stream, limit=LIMIT * 3):
    line = stream.readline(limit + 1)
    if not line or len(line) > limit or not line.endswith(b"\n"):
        raise RuntimeError("privilege channel closed or returned an invalid frame")
    return json.loads(line)


def authenticate(process, nonce, timeout=90):
    """Forward terminal keystrokes only until the authenticated ready receipt.

    Disable local echo and restore it on every exit path. Never collect a
    password string, log stdin, or keep terminal input in the ready buffer.
    """
    terminal = None
    saved = None
    pending = bytearray()
    deadline = time.monotonic() + timeout
    marker = json.dumps({"ready": nonce}).encode()
    prompt = (nonce + "]: ").encode()
    accepting_input = False
    try:
        try:
            terminal = os.open("/dev/tty", os.O_RDWR)
            saved = termios.tcgetattr(terminal)
            tty.setcbreak(terminal)
            settings = termios.tcgetattr(terminal)
            settings[3] &= ~termios.ECHO
            termios.tcsetattr(terminal, termios.TCSANOW, settings)
        except OSError:
            if terminal is not None:
                if saved is not None:
                    termios.tcsetattr(terminal, termios.TCSANOW, saved)
                os.close(terminal)
            terminal = None
        while time.monotonic() < deadline:
            sources = [process.stdout.fileno()]
            if terminal is not None and accepting_input:
                sources.append(terminal)
            ready, _, _ = select.select(sources, [], [], min(1, max(0, deadline - time.monotonic())))
            if process.stdout.fileno() in ready:
                chunk = os.read(process.stdout.fileno(), 4096)
                if not chunk:
                    raise RuntimeError("remote privilege authentication failed")
                pending.extend(chunk)
                if marker in pending:
                    # Broker emits nothing beyond readiness until our first RPC.
                    prefix = bytes(pending).split(marker, 1)[0]
                    if prefix:
                        os.write(terminal if terminal is not None else 2, prefix)
                    return
                if prompt in pending:
                    if terminal is None:
                        raise RuntimeError("sudo needs an interactive terminal before deployment")
                    # Sudo disables remote echo before displaying this prompt.
                    # Never forward pre-typed secrets before it is visible.
                    os.write(terminal if terminal is not None else 2, pending)
                    pending.clear()
                    accepting_input = True
                # Bound untrusted banners and display them without buffering
                # terminal input. Keep only the possible partial marker suffix.
                if len(pending) > len(marker):
                    count = len(pending) - len(marker)
                    os.write(terminal if terminal is not None else 2, pending[:count])
                    del pending[:count]
            if terminal is not None and terminal in ready:
                # One keystroke at a time; no password is assembled or persisted.
                key = os.read(terminal, 1)
                if not key:
                    raise RuntimeError("authentication terminal closed")
                process.stdin.write(key)
                process.stdin.flush()
        raise RuntimeError("remote authentication did not complete; no deployment started")
    finally:
        if terminal is not None:
            if saved is not None:
                termios.tcsetattr(terminal, termios.TCSANOW, saved)
            os.close(terminal)


def serve(args):
    path = Path(args.socket)
    if (not path.parent.is_dir() or path.parent.is_symlink()
            or path.parent.stat().st_mode & 0o077
            or path.parent.stat().st_uid != os.getuid()
            or Path(args.ready).parent != path.parent):
        raise RuntimeError("privilege channel requires an owned private parent directory")
    nonce = str(uuid.uuid4())
    prompt = f"Deployment sudo password for %u@%h [{nonce}]: "
    command = "exec sudo -p " + shlex.quote(prompt) + " -- python3 -u -c " + shlex.quote(BROKER) + " " + shlex.quote(nonce)
    process = subprocess.Popen(
        ["ssh", *args.ssh_option, "-o", "BatchMode=yes", "-o", "ConnectTimeout=10",
         "-o", "ServerAliveInterval=10", "-o", "ServerAliveCountMax=2", "-tt", args.host, command],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    listener = socket.socket(socket.AF_UNIX)
    bound = False
    ready_created = False
    try:
        authenticate(process, nonce)
        listener.bind(str(path))
        bound = True
        os.chmod(path, 0o600)
        listener.listen(1)
        listener.settimeout(0.5)
        fd = os.open(args.ready, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        os.close(fd)
        ready_created = True
        while process.poll() is None:
            try:
                client, _ = listener.accept()
            except socket.timeout:
                continue
            with client, client.makefile("rwb") as stream:
                client.settimeout(10)  # local request framing, not transaction time
                request = read_frame(stream, LIMIT)
                if request.get("op") != "run" or not isinstance(request.get("script"), str):
                    raise RuntimeError("invalid deployment command")
                process.stdin.write(json.dumps(request).encode() + b"\n")
                process.stdin.flush()
                response = read_frame(process.stdout)
                stream.write(json.dumps(response).encode() + b"\n")
                stream.flush()
        raise RuntimeError("remote privileged session ended; refusing further deployment")
    finally:
        listener.close()
        if bound:
            path.unlink(missing_ok=True)
        if ready_created:
            Path(args.ready).unlink(missing_ok=True)
        # Close the command stream first. No new command can be admitted. The
        # remote broker exits on EOF; active transactions remain responsible for
        # their canonical completion/rollback even if transport disappears.
        try:
            process.stdin.close()
        except BrokenPipeError:
            pass
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        process.stdout.close()


def run(args):
    with socket.socket(socket.AF_UNIX) as client:
        client.connect(args.socket)
        # The trusted CLI supplies scripts via stdin, never credentials/argv.
        script = sys.stdin.buffer.read(LIMIT + 1)
        if len(script) > LIMIT:
            raise RuntimeError("deployment command exceeds channel limit")
        with client.makefile("rwb") as stream:
            stream.write(json.dumps({"op": "run", "script": script.decode()}).encode() + b"\n")
            stream.flush()
            result = read_frame(stream)
    sys.stdout.buffer.write(base64.b64decode(result["stdout"], validate=True))
    sys.stdout.buffer.flush()
    sys.stderr.buffer.write(base64.b64decode(result["stderr"], validate=True))
    sys.stderr.buffer.flush()
    if result.get("truncated"):
        print("deployment command output exceeded channel limit", file=sys.stderr)
    code = result["code"]
    return code if 0 <= code <= 255 else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    server = commands.add_parser("serve")
    server.add_argument("--host", required=True)
    server.add_argument("--socket", required=True)
    server.add_argument("--ready", required=True)
    server.add_argument("--ssh-option", action="append", default=[])
    client = commands.add_parser("run")
    client.add_argument("--socket", required=True)
    args = parser.parse_args()
    if args.action == "run":
        return run(args)
    def stop(_signal, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    serve(args)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(130)
    except (OSError, ValueError, RuntimeError) as error:
        print(f"deployment privilege channel: {error}", file=sys.stderr)
        sys.exit(1)
