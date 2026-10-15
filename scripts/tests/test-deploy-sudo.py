#!/usr/bin/env python3
"""No real sudo, SSH, passwords, or target changes: isolated executable mocks."""
import fcntl
import os
from pathlib import Path
import pty
import select
import subprocess
import time
import tempfile
import termios
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/dist/deploy-sudo.sh"


class SudoLeaseTests(unittest.TestCase):
    def run_case(self, body, *, expected=0, password=False):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            mock = root / "sudo"
            mock.write_text('''#!/usr/bin/env bash
set -eu
printf 'sudo %s\\n' "$*" >> "$MOCK_LOG"
if [[ -f "$MOCK_ROOT/block" ]]; then
    printf '%s\\n' "$$" > "$MOCK_ROOT/refresher"
    exec sleep 30
fi
if [[ "$*" == -v ]]; then touch "$MOCK_ROOT/authorized"; exit 0; fi
[[ ! -f "$MOCK_ROOT/fail" ]]
''')
            mock.chmod(0o755)
            # Emulate sudo granting privilege to ONE SSH terminal. A new SSH
            # session has no ticket, even after a successful terminal login.
            (root / "ssh").write_text('''#!/usr/bin/env python3
import os,shlex,sys
from pathlib import Path
root=Path(os.environ["MOCK_ROOT"])
with open(os.environ["MOCK_LOG"],"a") as log:
    log.write("ssh " + " ".join(sys.argv[1:-1]) + "\\n")
with open(root/"ssh-pids","a") as log:
    log.write(str(os.getpid())+"\\n")
if (root/"bad-remote").exists(): raise SystemExit(1)
command=shlex.split(sys.argv[-1])
if command[:2] != ["exec","sudo"]: raise SystemExit(1)
code=command[command.index("-c")+1]
nonce=command[-1]
if os.environ.get("MOCK_ASK_PASSWORD") == "1":
    print(command[command.index("-p")+1],end="",flush=True)
    if sys.stdin.readline().strip() != "synthetic-password": raise SystemExit(1)
sys.argv=["synthetic-root-broker",nonce]
os.geteuid=lambda:0  # test-only fake sudo; no real privilege escalation
exec(code)
''')
            (root / "ssh").chmod(0o755)
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                       MOCK_ROOT=str(root), MOCK_LOG=str(root / "calls"),
                       MOCK_ASK_PASSWORD=str(int(password)))
            script = '''set -euo pipefail
source "$1"
ROOT_DIR="$2"
DRY_RUN=false
H_NAME=(local remote)
H_LOCAL=(true false)
H_SSH=('' example.invalid)
OQTO_SSH_OPTIONS=()
log() { :; }
err() { printf '%s\\n' "$*" >&2; }
host_exec() {
    printf 'lane %s\\n' "$1" >> "$MOCK_LOG"
    [[ ! -f "$MOCK_ROOT/bad-lane" ]]
}
trap stop_sudo_leases EXIT
SUDO_REFRESH_SECONDS=0.05
''' + body
            # Supply a controlling tty only for testing the mocked remote
            # interactive fallback's /dev/tty redirection. No input is read.
            master, slave = pty.openpty()
            def tty():
                os.setsid()
                fcntl.ioctl(0, termios.TIOCSCTTY, 0)
            before = termios.tcgetattr(slave)
            terminal_output = bytearray()
            try:
                command = ["bash", "-c", script, "test", str(HELPER), str(ROOT)]
                if password:
                    process = subprocess.Popen(command, stdin=slave, stdout=subprocess.PIPE,
                                               stderr=subprocess.PIPE, text=True,
                                               env=env, preexec_fn=tty)
                    deadline = time.monotonic() + 10
                    sent = False
                    while process.poll() is None and time.monotonic() < deadline:
                        ready, _, _ = select.select([master], [], [], 0.02)
                        if ready:
                            terminal_output.extend(os.read(master, 4096))
                            if b"]: " in terminal_output and not sent:
                                os.write(master, b"synthetic-password\n")
                                sent = True
                    stdout, stderr = process.communicate(timeout=1)
                    result = subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
                    self.assertTrue(sent, "authentication prompt was not displayed")
                else:
                    result = subprocess.run(command, stdin=slave, capture_output=True, text=True,
                                            env=env, preexec_fn=tty, timeout=10)
                self.assertEqual(termios.tcgetattr(slave), before, "terminal state not restored")
            finally:
                os.close(slave)
                os.close(master)
            calls = (root / "calls").read_text() if (root / "calls").exists() else ""
            self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
            self.assertNotIn("unbound variable", result.stderr)
            self.assertNotIn("synthetic-password", result.stdout + result.stderr + calls)
            self.assertNotIn(b"synthetic-password", terminal_output)
            for file in ("workers", "refresher", "ssh-pids"):
                if (root / file).exists():
                    for pid in (root / file).read_text().split():
                        with self.assertRaises(ProcessLookupError, msg=f"orphan process {pid}"):
                            os.kill(int(pid), 0)
            return calls, result.stderr

    def test_local_authenticates_once_refreshes_and_reaps(self):
        calls, _ = self.run_case('''start_sudo_leases 0
pids=("${SUDO_REFRESH_PIDS[@]}")
dir="$SUDO_LEASE_DIR"
sleep 0.2
require_sudo_leases
stop_sudo_leases
[[ ! -e "$dir" ]]
for pid in "${pids[@]}"; do ! kill -0 "$pid" 2>/dev/null; done
''')
        self.assertEqual(calls.splitlines().count("sudo -v"), 1)
        self.assertGreaterEqual(calls.splitlines().count("sudo -n -v"), 2)

    def test_every_selected_host_is_authorized_before_build(self):
        calls, _ = self.run_case('''start_sudo_leases 0 1
printf 'build\\n' >> "$MOCK_LOG"
require_sudo_leases
''')
        self.assertLess(calls.index("lane true"), calls.index("build"))
        self.assertLess(calls.index("ssh "), calls.index("build"))
        self.assertEqual(sum("-tt" in line for line in calls.splitlines()), 1)

    def test_tty_scoped_remote_keeps_one_session_across_commands(self):
        calls, _ = self.run_case('''start_sudo_leases 1
sleep 0.15
require_sudo_leases
''')
        self.assertEqual(sum("-tt" in line for line in calls.splitlines()), 1)
        self.assertIn("BatchMode=yes", calls)
        self.assertEqual(sum(line.startswith("ssh ") for line in calls.splitlines()), 1)

    def test_remote_password_is_terminal_only_and_not_echoed_or_logged(self):
        calls, _ = self.run_case('''start_sudo_leases 1
remote_privileged_exec example.invalid 'printf synthetic-result'
''', password=True)
        self.assertEqual(sum(line.startswith("ssh ") for line in calls.splitlines()), 1)

    def test_remote_does_not_depend_on_a_second_sudo_ticket(self):
        calls, _ = self.run_case('''touch "$MOCK_ROOT/bad-lane"
start_sudo_leases 1
remote_privileged_exec example.invalid 'printf first'
sleep 0.15
remote_privileged_exec example.invalid 'printf second'
''')
        self.assertEqual(sum(line.startswith("ssh ") for line in calls.splitlines()), 1)
        self.assertNotIn("lane false", calls)

    def test_remote_preserves_output_exit_code_and_large_scripts(self):
        self.run_case('''start_sudo_leases 1
if remote_privileged_exec example.invalid 'printf out; printf err >&2; exit 7'; then exit 1; else [[ $? == 7 ]]; fi
payload="$(printf '%05000d' 0)"
output="$(remote_privileged_exec example.invalid "printf '%s' '$payload'")"
[[ "$output" == "$payload" ]]
''')

    def test_closed_remote_session_blocks_further_actions(self):
        self.run_case('''start_sudo_leases 1
kill "${SUDO_REFRESH_PIDS[0]}"
wait "${SUDO_REFRESH_PIDS[0]}" || true
if require_sudo_leases; then exit 1; fi
if remote_privileged_exec example.invalid true; then exit 1; fi
''')

    def test_disconnect_does_not_kill_an_inflight_transaction(self):
        self.run_case('''start_sudo_leases 1
remote_privileged_exec example.invalid "touch '$MOCK_ROOT/started'; sleep 0.2; touch '$MOCK_ROOT/completed'" & client=$!
for ((i=0; i<100; i++)); do
    [[ ! -f "$MOCK_ROOT/started" ]] || break
    sleep 0.01
done
[[ -f "$MOCK_ROOT/started" ]]
kill "${SUDO_REFRESH_PIDS[0]}"
wait "${SUDO_REFRESH_PIDS[0]}" || true
wait "$client" || true
[[ -f "$MOCK_ROOT/completed" ]]
''')

    def test_failed_refresh_blocks_actions_without_reprompt(self):
        calls, error = self.run_case('''start_sudo_leases 0
touch "$MOCK_ROOT/fail"
for ((i=0; i<100; i++)); do
    [[ ! -f "$SUDO_LEASE_DIR/failed" ]] || break
    sleep 0.01
done
[[ -f "$SUDO_LEASE_DIR/failed" ]]
if require_sudo_leases; then exit 1; fi
''')
        self.assertEqual(calls.splitlines().count("sudo -v"), 1)
        self.assertIn("without another password prompt", error)

    def test_stopped_worker_blocks_actions(self):
        self.run_case('''start_sudo_leases 0
kill "${SUDO_REFRESH_PIDS[0]}"
wait "${SUDO_REFRESH_PIDS[0]}" || true
if require_sudo_leases; then exit 1; fi
''')

    def test_exit_after_partial_authentication_reaps_workers(self):
        self.run_case('''trap 'printf "%s\\n" "${SUDO_REFRESH_PIDS[@]}" > "$MOCK_ROOT/workers"; stop_sudo_leases' EXIT
touch "$MOCK_ROOT/bad-remote"
start_sudo_leases 0 1
''', expected=1)

    def test_signal_reaps_worker_and_inflight_refresh(self):
        self.run_case('''trap 'exit 143' TERM
start_sudo_leases 0
printf '%s\\n' "${SUDO_REFRESH_PIDS[@]}" > "$MOCK_ROOT/workers"
touch "$MOCK_ROOT/block"
for ((i=0; i<100; i++)); do
    [[ ! -f "$MOCK_ROOT/refresher" ]] || break
    sleep 0.01
done
[[ -f "$MOCK_ROOT/refresher" ]]
kill -TERM $$
''', expected=143)

    def test_dry_run_does_not_authenticate_or_create_workers(self):
        calls, _ = self.run_case('''DRY_RUN=true
start_sudo_leases 0 1
require_sudo_leases
[[ -z "$SUDO_LEASE_DIR" && ${#SUDO_REFRESH_PIDS[@]} == 0 ]]
''')
        self.assertEqual(calls, "")

    def test_driver_orders_auth_before_build_and_keeps_checks(self):
        source = (ROOT / "scripts/deploy.sh").read_text()
        main = source.split("# --- main", 1)[1]
        self.assertLess(main.index('start_sudo_leases "${selected[@]}"'),
                        main.index("    build_artifact"))
        self.assertIn("trap stop_sudo_leases EXIT", main)
        self.assertIn("trap 'exit 130' INT", main)
        self.assertIn("trap 'exit 143' TERM", main)
        privileged = source.split("host_exec_sudo() {", 1)[1].split("\n}", 1)[0]
        self.assertIn("require_sudo_leases || return 1", privileged)
        self.assertIn("sudo -n bash", privileged)
        self.assertNotIn("prime_sudo", source)
        sync = source.split("sync_extensions_host() {", 1)[1].split("\n}", 1)[0]
        self.assertIn("require_sudo_leases || return 1", sync)
        self.assertEqual(sync.count("--skip-pi --non-interactive"), 2)


if __name__ == "__main__":
    unittest.main()
