#!/usr/bin/env bash
# Scenario 08: no_new_privs.
#
# When no_new_privs=true, setuid/setgid binaries must not gain privileges inside
# the sandbox. We detect this by inspecting /proc/self/status for NoNewPrivs:1.
# We do not attempt to exec a setuid binary to avoid relying on host details.

set -euo pipefail
SCENARIO_NAME="08-no-new-privs"
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
trap cleanup_run_dir EXIT
scenario_header
require_oqto_sandbox
have_bwrap || { skip "bwrap missing"; scenario_summary; exit 0; }

ws="$(make_workspace nnp)"

make_cfg() {
  local flag="$1"
  local cfg="${OQTO_TEST_RUN_DIR}/nnp-${flag}.toml"
  write_toml "${cfg}" <<EOF
enabled = true
profile = "nnp-${flag}"

[profiles.nnp-${flag}]
deny_read = []
allow_write = ["/tmp"]
deny_write = []
isolate_network = false
isolate_pid = false
drop_all_caps = false
disable_userns = false
assert_userns_disabled = false
no_new_privs = ${flag}
seccomp_mode = "off"
landlock_mode = "off"
EOF
  echo "${cfg}"
}

cfg_on="$(make_cfg true)"
out_on="$(run_sandbox --config "${cfg_on}" --workspace "${ws}" -- \
  sh -c 'grep ^NoNewPrivs /proc/self/status')"
case "${out_on}" in
  *"NoNewPrivs:"*"1"*) pass "no_new_privs=true -> NoNewPrivs:1";;
  *) fail "no_new_privs=true did not set NoNewPrivs (got: ${out_on})";;
esac

cfg_off="$(make_cfg false)"
out_off="$(run_sandbox --config "${cfg_off}" --workspace "${ws}" -- \
  sh -c 'grep ^NoNewPrivs /proc/self/status')"
# bwrap itself may still set NoNewPrivs=1 depending on its own hardening; we
# only care that our flag is not misreported. Accept either 0 or 1 here and
# document the finding.
case "${out_off}" in
  *"NoNewPrivs:"*"0"*) pass "no_new_privs=false -> NoNewPrivs:0";;
  *"NoNewPrivs:"*"1"*) warn "no_new_privs=false but NoNewPrivs:1 (bwrap-driven default)"; pass "flag consistency acceptable";;
  *) fail "no_new_privs=false did not produce NoNewPrivs field (got: ${out_off})";;
esac

scenario_summary
