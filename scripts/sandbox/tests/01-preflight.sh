#!/usr/bin/env bash
# Scenario 01: preflight.
# Validates host/kernel capabilities the rest of the suite depends on.
# Failures here usually mean later scenarios will be skipped rather than buggy.

set -euo pipefail
SCENARIO_NAME="01-preflight"
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
trap cleanup_run_dir EXIT
scenario_header

# Writable run dir with enough free space (quota-exhaustion guard)
if check_run_dir_writable 20; then
  pass "run dir has >= 20MB free: ${OQTO_TEST_RUN_DIR}"
else
  fail "run dir out of space/quota: ${OQTO_TEST_RUN_DIR}"
fi

# Binary present
if bin="$(find_oqto_sandbox)"; then
  pass "oqto-sandbox binary: ${bin}"
else
  fail "oqto-sandbox binary not found"
fi

# bwrap present
if have_bwrap; then
  pass "bwrap present: $(bwrap --version | head -1)"
else
  fail "bwrap missing (install bubblewrap)"
fi

# User namespaces enabled
if have_user_namespaces; then
  pass "user namespaces enabled (max_user_namespaces=$(cat /proc/sys/user/max_user_namespaces))"
else
  fail "user namespaces disabled; bwrap will not work"
fi

# Landlock
if have_landlock; then
  pass "Landlock kernel support detected"
else
  skip "Landlock not supported by kernel (scenario 07 will skip enforcement asserts)"
fi

# Seccomp BPF file
if have_seccomp_bpf; then
  pass "Seccomp BPF present at ${OQTO_SECCOMP_BPF:-/etc/oqto/seccomp/default.bpf}"
else
  skip "Seccomp BPF missing; run scripts/sandbox/generate-seccomp-artifacts.sh && sudo scripts/sandbox/install-seccomp-policy.sh"
fi

# /proc/sys tunables relevant to seccomp
if [[ -r /proc/sys/kernel/seccomp ]] || [[ -r /proc/sys/kernel/seccomp/actions_avail ]]; then
  pass "kernel seccomp knobs readable"
else
  skip "kernel seccomp knobs not exposed (host-specific)"
fi

scenario_summary
