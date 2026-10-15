#!/usr/bin/env python3
"""Bounded real upstream TUI/PTY proof, not physical-terminal acceptance."""
import fcntl
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import termios
import time

ROOT = Path(__file__).resolve().parents[2]
CSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")

class Terminal:
    def __init__(self, args=None, cwd=None, env=None):
        self.started = time.monotonic()
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 112, 0, 0))
        self.process = subprocess.Popen([str(ROOT / "oqt"), *(args or ["--demo"])], cwd=cwd or ROOT, env=env,
            stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
        os.close(slave)
        self.raw = bytearray()
        self.deadline = time.monotonic() + 45

    def resize(self, rows, cols):
        fcntl.ioctl(self.master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        os.killpg(self.process.pid, signal.SIGWINCH)

    def send(self, text):
        os.write(self.master, text.encode())

    def expect(self, text, after=0):
        while time.monotonic() < self.deadline:
            clean = CSI.sub(b"", self.raw[after:]).decode("utf8", "replace")
            if text in clean:
                return len(self.raw)
            if self.process.poll() is not None:
                raise AssertionError(f"TUI exited {self.process.returncode}: {clean[-2500:]}")
            if select.select([self.master], [], [], .1)[0]:
                data = os.read(self.master, 65536)
                self.raw.extend(data)
                if b"\x1b[6n" in data:
                    os.write(self.master, b"\x1b[1;1R")
        raise AssertionError(f"TUI deadline waiting for {text!r}: {clean[-2500:]}")

    def close(self, label):
        self.send("\x03"); time.sleep(.15); self.send("\x03")
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(self.process.pid, signal.SIGTERM)
            self.process.wait(timeout=3)
        finally:
            os.close(self.master)
            Path(f"/tmp/oqt-{label}-pty.log").write_bytes(self.raw)


def scenario(prompt, choice, result, label, revoke=False):
    terminal = Terminal()
    try:
        terminal.expect("faux/faux-1")
        print(f"TUI ready ({label}): {time.monotonic()-terminal.started:.3f}s", flush=True)
        terminal.expect("oqt DEMO")
        marker = len(terminal.raw)
        terminal.send(prompt + "\r")
        terminal.expect("DEMO permission", marker)
        terminal.expect("→ Deny", marker)
        # Safest default is Deny. Once is two Up presses; Session is one.
        terminal.send({"once":"\x1b[A\x1b[A\r", "session":"\x1b[A\r", "deny":"\r"}[choice])
        terminal.expect(result, marker)
        # Usage arrows appear in the classic-style footer once the turn settled.
        terminal.expect("\u2191", marker)
        if revoke:
            marker = len(terminal.raw)
            terminal.send("/permissions revoke\r")
            terminal.expect("Approved overlays revoked", marker)
            marker = len(terminal.raw)
            terminal.send(prompt + "\r")
            terminal.expect("DEMO permission", marker)
            terminal.expect("→ Deny", marker)
            terminal.send("\r")
            terminal.expect("Permission denied", marker)
        print(f"PASS {label}: real TUI + in-turn {choice}" + (" + live revoke/re-request" if revoke else ""))
    finally:
        terminal.close(label)

if __name__ == '__main__':
    scenario("global history", "once", "broader fixture-token", "history-once")
    scenario("read file", "deny", "Permission denied", "file-deny")
    scenario("read file", "session", "confined hands returned", "file-session-revoke", revoke=True)
