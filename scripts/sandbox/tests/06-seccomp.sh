#!/usr/bin/env bash
# Scenario 06: seccomp modes.
#
# off     -> bwrap args contain no --seccomp; no warning logged.
# audit   -> with BPF present: --seccomp 3 wired up. without: warning logged, sandbox still runs.
# enforce -> with BPF present: --seccomp 3 wired. without: build_bwrap_args returns None
#            and the CLI aborts. We assert exit != 0.
#
# Requires the BPF artifact for the positive enforce path. If missing, we run
# the no-file arm and SKIP the positive arms with a pointer to the install script.

set -euo pipefail
SCENARIO_NAME="06-seccomp"
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
trap cleanup_run_dir EXIT
scenario_header
require_oqto_sandbox
have_bwrap || { skip "bwrap missing"; scenario_summary; exit 0; }

ws="$(make_workspace seccomp)"
bpf="${OQTO_SECCOMP_BPF:-/etc/oqto/seccomp/default.bpf}"

make_cfg() {
  # $1 = seccomp_mode  $2 = include_bpf_path (true|false)
  local mode="$1"
  local include_bpf="$2"
  local cfg="${OQTO_TEST_RUN_DIR}/seccomp-${mode}-${include_bpf}.toml"
  {
    cat <<EOF
enabled = true
profile = "sc-${mode}-${include_bpf}"

[profiles.sc-${mode}-${include_bpf}]
deny_read = []
allow_write = ["/tmp"]
deny_write = []
isolate_network = false
isolate_pid = false
drop_all_caps = false
disable_userns = false
assert_userns_disabled = false
no_new_privs = true
seccomp_mode = "${mode}"
landlock_mode = "off"
EOF
    if [[ "${include_bpf}" == "true" ]]; then
      echo "seccomp_bpf_path = \"${bpf}\""
    fi
  } >"${cfg}"
  echo "${cfg}"
}

# ----- off: --seccomp absent, no warning -----
cfg_off="$(make_cfg off false)"
out_off="$(run_sandbox --config "${cfg_off}" --workspace "${ws}" --dry-run -- /bin/true 2>&1)"
if [[ "${out_off}" == *"--seccomp"* ]]; then
  fail "seccomp=off should not emit --seccomp"
else
  pass "seccomp=off: --seccomp absent"
fi
if [[ "${out_off}" == *"seccomp_bpf_path missing/unreadable"* ]]; then
  fail "seccomp=off should not emit missing-path warning"
else
  pass "seccomp=off: no missing-path warning"
fi

# ----- audit without BPF: warning logged, sandbox still runs -----
cfg_audit_nobpf="$(make_cfg audit false)"
out_audit_nobpf="$(run_sandbox --config "${cfg_audit_nobpf}" --workspace "${ws}" --dry-run -- /bin/true 2>&1)"
if [[ "${out_audit_nobpf}" == *"seccomp_bpf_path missing/unreadable"* ]]; then
  pass "seccomp=audit (no bpf): warns as expected"
else
  fail "seccomp=audit (no bpf): missing warning line"
fi
if [[ "${out_audit_nobpf}" == *"--seccomp"* ]]; then
  fail "seccomp=audit (no bpf): should not wire --seccomp"
else
  pass "seccomp=audit (no bpf): --seccomp absent"
fi

# ----- enforce without BPF: CLI must abort (bwrap args = None) -----
cfg_enf_nobpf="$(make_cfg enforce false)"
# exit code may be 1 depending on anyhow mapping; just assert non-zero.
rc=0
run_sandbox --config "${cfg_enf_nobpf}" --workspace "${ws}" --dry-run -- /bin/true >/dev/null 2>&1 || rc=$?
if [[ "${rc}" -ne 0 ]]; then
  pass "seccomp=enforce without bpf: CLI aborts (rc=${rc})"
else
  fail "seccomp=enforce without bpf: CLI unexpectedly succeeded"
fi

# ----- audit with BPF: --seccomp 3 present -----
if have_seccomp_bpf; then
  cfg_audit_bpf="$(make_cfg audit true)"
  out_audit_bpf="$(run_sandbox --config "${cfg_audit_bpf}" --workspace "${ws}" --dry-run -- /bin/true 2>&1)"
  if [[ "${out_audit_bpf}" == *"--seccomp"*" 3"* ]]; then
    pass "seccomp=audit + bpf: --seccomp 3 wired"
  else
    fail "seccomp=audit + bpf: --seccomp not wired (check BPF readability)"
  fi

  # ----- enforce with BPF: real run -----
  cfg_enf_bpf="$(make_cfg enforce true)"
  assert_success \
    "seccomp=enforce + bpf: harmless command still runs" \
    -- run_sandbox --config "${cfg_enf_bpf}" --workspace "${ws}" -- /bin/true

  # Negative assertion is policy-dependent: rather than guessing which syscall
  # the default policy forbids, we just confirm the sandbox starts and exits
  # normally. Extend here once a known-denied syscall test helper is added.
else
  skip "seccomp BPF missing; run scripts/sandbox/generate-seccomp-artifacts.sh && sudo scripts/sandbox/install-seccomp-policy.sh"
fi

scenario_summary
