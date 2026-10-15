#!/usr/bin/env python3
"""No real sudo, SSH, passwords, or target changes: isolated executable mocks."""
import fcntl
import os
from pathlib import Path
import pty
import subprocess
import tempfile
import termios
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/dist/deploy-sudo.sh"


class SudoLeaseTests(unittest.TestCase):
    def run_case(self, body, *, remote_prompt=False, expected=0):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in ("sudo", "ssh"):
                mock = root / name
                mock.write_text('''#!/usr/bin/env bash
set -eu
printf '%s %s\\n' "${0##*/}" "$*" >> "$MOCK_LOG"
if [[ -f "$MOCK_ROOT/block" ]]; then
    printf '%s\\n' "$$" > "$MOCK_ROOT/refresher"
    exec sleep 30
fi
case "${0##*/}" in
sudo)
    if [[ "$*" == -v ]]; then touch "$MOCK_ROOT/authorized"; exit 0; fi
    [[ ! -f "$MOCK_ROOT/fail" ]] || exit 1
    ;;
ssh)
    if [[ "$*" == *-tt* ]]; then touch "$MOCK_ROOT/authorized"; exit 0; fi
    [[ ! -f "$MOCK_ROOT/fail" ]] || exit 1
    if [[ "${MOCK_REQUIRE_PROMPT:-0}" == 1 ]]; then [[ -f "$MOCK_ROOT/authorized" ]] || exit 1; fi
    ;;
esac
''')
                mock.chmod(0o755)
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                       MOCK_ROOT=str(root), MOCK_LOG=str(root / "calls"),
                       MOCK_REQUIRE_PROMPT=str(int(remote_prompt)))
            script = '''set -euo pipefail
source "$1"
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
            try:
                result = subprocess.run(["bash", "-c", script, "test", str(HELPER)],
                                        stdin=slave, capture_output=True, text=True,
                                        env=env, preexec_fn=tty, timeout=10)
            finally:
                os.close(slave)
                os.close(master)
            calls = (root / "calls").read_text() if (root / "calls").exists() else ""
            self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
            self.assertNotIn("unbound variable", result.stderr)
            for file in ("workers", "refresher"):
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
        self.assertLess(calls.index("lane false"), calls.index("build"))
        self.assertNotIn("-tt", calls)

    def test_remote_prompt_happens_once_only_at_start(self):
        calls, _ = self.run_case('''start_sudo_leases 1
sleep 0.15
require_sudo_leases
''', remote_prompt=True)
        self.assertEqual(sum("-tt" in line for line in calls.splitlines()), 1)
        self.assertIn("BatchMode=yes", calls)

    def test_incompatible_remote_lane_fails_before_build(self):
        calls, error = self.run_case('''touch "$MOCK_ROOT/bad-lane"
if start_sudo_leases 1; then exit 1; fi
[[ ${#SUDO_REFRESH_PIDS[@]} == 0 ]]
''', remote_prompt=True)
        self.assertIn("refusing before build", error)
        self.assertNotIn("build\n", calls)

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
host_exec() { [[ "$1" == true ]]; }
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
