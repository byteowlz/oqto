#!/usr/bin/env bash
# Scenario 04: network isolation.
# isolate_network=true  -> no network namespace access (DNS + TCP must fail)
# isolate_network=false -> network reachable (assumes host has network; falls back to skip)

set -euo pipefail
SCENARIO_NAME="04-network-isolation"
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
trap cleanup_run_dir EXIT
scenario_header
require_oqto_sandbox
have_bwrap || { skip "bwrap missing"; scenario_summary; exit 0; }

ws="$(make_workspace net)"

make_cfg() {
  local isolate="$1"
  local cfg="${OQTO_TEST_RUN_DIR}/net-${isolate}.toml"
  write_toml "${cfg}" <<EOF
enabled = true
profile = "net-${isolate}"

[profiles.net-${isolate}]
deny_read = []
allow_write = ["/tmp"]
deny_write = []
isolate_network = ${isolate}
isolate_pid = false
drop_all_caps = false
disable_userns = false
assert_userns_disabled = false
no_new_privs = false
seccomp_mode = "off"
landlock_mode = "off"
EOF
  echo "${cfg}"
}

# --- isolated: loopback only ---
cfg_iso="$(make_cfg true)"
# Read /proc/net/route -- when net is unshared, route table is empty/minimal.
assert_stdout_not_contains \
  "isolate_network=true kills default route" \
  "00000000" \
  -- run_sandbox --config "${cfg_iso}" --workspace "${ws}" -- cat /proc/net/route

# Loopback should still exist (unshare brings up lo).
assert_stdout_contains \
  "isolate_network=true keeps loopback interface" \
  "lo" \
  -- run_sandbox --config "${cfg_iso}" --workspace "${ws}" -- cat /proc/net/dev

# Outbound connect must fail. Use /dev/tcp trick via bash.
assert_failure \
  "isolate_network=true blocks outbound TCP to 1.1.1.1:53" \
  -- run_sandbox --config "${cfg_iso}" --workspace "${ws}" -- \
  bash -c 'exec 3<>/dev/tcp/1.1.1.1/53'

# --- open: expect at least loopback plus default route (if host has one) ---
cfg_open="$(make_cfg false)"
if ip route | grep -q '^default'; then
  assert_stdout_contains \
    "isolate_network=false preserves default route" \
    "00000000" \
    -- run_sandbox --config "${cfg_open}" --workspace "${ws}" -- cat /proc/net/route
else
  skip "host has no default route; cannot verify open-network positive case"
fi

scenario_summary
