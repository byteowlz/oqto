#!/usr/bin/env bash
# Scenario 05: PID isolation.
# isolate_pid=true  -> sandboxed /proc sees only the sandbox's own tree (PID 1 is our init)
# isolate_pid=false -> host PIDs visible

set -euo pipefail
SCENARIO_NAME="05-pid-isolation"
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
trap cleanup_run_dir EXIT
scenario_header
require_oqto_sandbox
have_bwrap || { skip "bwrap missing"; scenario_summary; exit 0; }

ws="$(make_workspace pid)"

make_cfg() {
  local isolate="$1"
  local cfg="${OQTO_TEST_RUN_DIR}/pid-${isolate}.toml"
  write_toml "${cfg}" <<EOF
enabled = true
profile = "pid-${isolate}"

[profiles.pid-${isolate}]
deny_read = []
allow_write = ["/tmp"]
deny_write = []
isolate_network = false
isolate_pid = ${isolate}
drop_all_caps = false
disable_userns = false
assert_userns_disabled = false
no_new_privs = false
seccomp_mode = "off"
landlock_mode = "off"
EOF
  echo "${cfg}"
}

# --- isolated: /proc shows a short list; in particular no host init process ---
cfg_iso="$(make_cfg true)"
host_pid_count=$(ls /proc 2>/dev/null | grep -c '^[0-9]\+$' || echo 0)
sandbox_pid_count="$(run_sandbox --config "${cfg_iso}" --workspace "${ws}" -- \
  sh -c 'ls /proc 2>/dev/null | grep -c "^[0-9]\+$"' | tr -d '[:space:]')"

if [[ -z "${sandbox_pid_count}" ]]; then
  fail "could not count PIDs inside isolated sandbox"
elif [[ "${sandbox_pid_count}" -lt "${host_pid_count}" && "${sandbox_pid_count}" -lt 20 ]]; then
  pass "isolate_pid=true reduces visible PIDs (host=${host_pid_count}, sandbox=${sandbox_pid_count})"
else
  fail "isolate_pid=true did not reduce visible PIDs (host=${host_pid_count}, sandbox=${sandbox_pid_count})"
fi

# Under --unshare-pid bwrap becomes PID 1, so our shell typically sees a very small tree.
assert_stdout_contains \
  "isolated sandbox has a /proc/1" \
  "/proc/1" \
  -- run_sandbox --config "${cfg_iso}" --workspace "${ws}" -- \
  sh -c 'ls -d /proc/1'

# --- open: host PIDs visible ---
cfg_open="$(make_cfg false)"
open_pid_count="$(run_sandbox --config "${cfg_open}" --workspace "${ws}" -- \
  sh -c 'ls /proc 2>/dev/null | grep -c "^[0-9]\+$"' | tr -d '[:space:]')"

if [[ -n "${open_pid_count}" && "${open_pid_count}" -gt 20 ]]; then
  pass "isolate_pid=false preserves host PID view (count=${open_pid_count})"
else
  fail "isolate_pid=false unexpectedly small PID view (count=${open_pid_count})"
fi

scenario_summary
