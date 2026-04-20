#!/usr/bin/env bash
# Scenario 07: Landlock modes.
#
# Landlock is now applied by an inner shim (crate::shim) AFTER bwrap completes
# user-namespace setup, so disable_userns=true no longer bypasses it. trx
# oqto-b4za is fixed. This scenario verifies both paths:
#
#   off                                              -> writes anywhere succeed (bwrap only)
#   enforce + disable_userns=false                   -> writes outside allow_write blocked
#   enforce + disable_userns=true  (formerly broken) -> ALSO blocked (regression guard)

set -euo pipefail
SCENARIO_NAME="07-landlock"
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
trap cleanup_run_dir EXIT
scenario_header
require_oqto_sandbox
have_bwrap || { skip "bwrap missing"; scenario_summary; exit 0; }

if ! have_landlock; then
  skip "kernel does not support Landlock; nothing to exercise"
  scenario_summary
  exit 0
fi

ws="$(make_workspace landlock)"
# Writable allow_write dir; and a target path we expect Landlock to block.
allowed_dir="${OQTO_TEST_RUN_DIR}/ll-allowed"
# A path that bwrap bind-mounts rw (home) but Landlock must deny.
victim_dir="${HOME}/.oqto-landlock-victim"
mkdir -p "${allowed_dir}"

cleanup_victim() {
  rm -rf "${victim_dir}" 2>/dev/null || true
}
trap "cleanup_victim; cleanup_run_dir" EXIT

mkdir -p "${victim_dir}"

make_cfg() {
  # $1 = landlock_mode  $2 = disable_userns (true|false)
  local mode="$1"
  local dis_userns="$2"
  local cfg="${OQTO_TEST_RUN_DIR}/ll-${mode}-dus${dis_userns}.toml"
  write_toml "${cfg}" <<EOF
enabled = true
profile = "ll-${mode}-${dis_userns}"

[profiles.ll-${mode}-${dis_userns}]
deny_read = []
allow_write = ["${allowed_dir}", "/tmp"]
deny_write = []
isolate_network = false
isolate_pid = false
drop_all_caps = false
disable_userns = ${dis_userns}
assert_userns_disabled = false
no_new_privs = true
seccomp_mode = "off"
landlock_mode = "${mode}"
EOF
  echo "${cfg}"
}

# ----- off: bwrap alone. victim writable (since home is bind-mounted rw). -----
cfg_off="$(make_cfg off false)"
assert_success \
  "landlock=off: workspace writable" \
  -- run_sandbox --config "${cfg_off}" --workspace "${ws}" -- \
  sh -c "touch '${ws}/ok'"
assert_success \
  "landlock=off: allow_write writable" \
  -- run_sandbox --config "${cfg_off}" --workspace "${ws}" -- \
  sh -c "touch '${allowed_dir}/ok'"

# ----- enforce, disable_userns=false: Landlock actually runs -----
cfg_enf="$(make_cfg enforce false)"
assert_success \
  "landlock=enforce: workspace writable" \
  -- run_sandbox --config "${cfg_enf}" --workspace "${ws}" -- \
  sh -c "touch '${ws}/ok-enf'"
assert_success \
  "landlock=enforce: allow_write writable" \
  -- run_sandbox --config "${cfg_enf}" --workspace "${ws}" -- \
  sh -c "touch '${allowed_dir}/ok-enf'"

# The critical negative assertion: a home path NOT in allow_write must fail.
rm -f "${victim_dir}/tombstone"
assert_failure \
  "landlock=enforce: writes outside allow_write blocked" \
  -- run_sandbox --config "${cfg_enf}" --workspace "${ws}" -- \
  sh -c "touch '${victim_dir}/tombstone'"
if [[ -e "${victim_dir}/tombstone" ]]; then
  fail "landlock=enforce: tombstone leaked (Landlock did NOT block)"
else
  pass "landlock=enforce: tombstone not created"
fi

# ----- enforce, disable_userns=true: regression guard for oqto-b4za -----
# Under the shim fix, Landlock must still block writes even when --unshare-user
# + --disable-userns are active. If this assertion ever fails, the shim is not
# wired up correctly.
cfg_bug="$(make_cfg enforce true)"
rm -f "${victim_dir}/bug-tombstone"
assert_failure \
  "landlock=enforce + disable_userns=true: writes still blocked (oqto-b4za guard)" \
  -- run_sandbox --config "${cfg_bug}" --workspace "${ws}" -- \
  sh -c "touch '${victim_dir}/bug-tombstone'"
if [[ -e "${victim_dir}/bug-tombstone" ]]; then
  fail "landlock=enforce + disable_userns=true: tombstone leaked (oqto-b4za regressed)"
else
  pass "landlock=enforce + disable_userns=true: no tombstone (shim applied)"
fi

scenario_summary
