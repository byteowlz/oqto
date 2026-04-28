#!/usr/bin/env bash
# Orchestrator: runs every sandbox-hardening scenario and reports aggregate results.
#
# Usage:
#   scripts/sandbox/tests/run-all.sh              # run all scenarios
#   scripts/sandbox/tests/run-all.sh 03 07        # run selected scenarios (by number prefix)
#
# Env:
#   OQTO_SANDBOX_BIN       Override binary path (default: auto-discover)
#   OQTO_SECCOMP_BPF       Seccomp BPF file (default: /etc/oqto/seccomp/default.bpf)
#   OQTO_TEST_KEEP_DIR=1   Keep temp dir on exit for debugging
#   VERBOSE=1              Pass -v to oqto-sandbox in scenarios that accept it

set -euo pipefail

TESTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "${TESTS_DIR}/lib.sh"

trap cleanup_run_dir EXIT

SELECTORS=("$@")

want_scenario() {
  local name="$1"
  if [[ "${#SELECTORS[@]}" -eq 0 ]]; then
    return 0
  fi
  for sel in "${SELECTORS[@]}"; do
    if [[ "${name}" == ${sel}* ]]; then
      return 0
    fi
  done
  return 1
}

printf '%soqto-sandbox hardening test suite%s\n' "${C_BOLD}" "${C_OFF}"
printf 'run dir: %s\n' "${OQTO_TEST_RUN_DIR}"

if ! bin="$(find_oqto_sandbox)"; then
  printf '%sFATAL%s oqto-sandbox not found. Build: cargo build --release -p oqto-sandbox\n' \
    "${C_RED}" "${C_OFF}" >&2
  exit 2
fi
printf 'binary:  %s\n' "${bin}"

TOTAL_PASS=0
TOTAL_FAIL=0
TOTAL_SKIP=0
FAILED_SCENARIOS=()

PREFLIGHT_FAILED=0
for script in "${TESTS_DIR}"/[0-9][0-9]-*.sh; do
  [[ -e "${script}" ]] || continue
  name="$(basename "${script}" .sh)"
  prefix="${name%%-*}"

  if ! want_scenario "${prefix}"; then
    continue
  fi

  # Preflight failures (missing bwrap, no space) are catastrophic for later
  # scenarios; halt rather than cascade.
  if [[ "${PREFLIGHT_FAILED}" -eq 1 ]]; then
    printf '%sSKIP%s %s: preflight failed\n' "${C_YELLOW}" "${C_OFF}" "${name}"
    continue
  fi

  printf '\n%s--- running %s ---%s\n' "${C_BOLD}" "${name}" "${C_OFF}"
  # Each scenario runs in a subshell so `exit` in one doesn't kill the suite.
  rc=0
  out="$(bash "${script}" 2>&1)" || rc=$?
  printf '%s\n' "${out}"

  # Parse trailing "N passed, N failed, N skipped" line.
  summary_line="$(printf '%s' "${out}" | awk '/passed, .* failed, .* skipped/ {line=$0} END{print line}')"
  p="$(awk '{for(i=1;i<=NF;i++) if($i=="passed,") print $(i-1)}' <<<"${summary_line}")"
  f="$(awk '{for(i=1;i<=NF;i++) if($i=="failed,") print $(i-1)}' <<<"${summary_line}")"
  s="$(awk '{for(i=1;i<=NF;i++) if($i=="skipped") print $(i-1)}' <<<"${summary_line}")"
  TOTAL_PASS=$((TOTAL_PASS + ${p:-0}))
  TOTAL_FAIL=$((TOTAL_FAIL + ${f:-0}))
  TOTAL_SKIP=$((TOTAL_SKIP + ${s:-0}))

  if [[ "${rc}" -ne 0 || "${f:-0}" -gt 0 ]]; then
    FAILED_SCENARIOS+=("${name}")
    if [[ "${name}" == 01-preflight ]]; then
      PREFLIGHT_FAILED=1
    fi
  fi
done

printf '\n%s==== suite summary ====%s\n' "${C_BOLD}" "${C_OFF}"
printf 'passed:  %d\nfailed:  %d\nskipped: %d\n' "${TOTAL_PASS}" "${TOTAL_FAIL}" "${TOTAL_SKIP}"

if [[ "${#FAILED_SCENARIOS[@]}" -gt 0 ]]; then
  printf '%sfailed scenarios:%s\n' "${C_RED}" "${C_OFF}"
  for s in "${FAILED_SCENARIOS[@]}"; do
    printf '  - %s\n' "${s}"
  done
  exit 1
fi

printf '%sall scenarios green%s\n' "${C_GREEN}" "${C_OFF}"
