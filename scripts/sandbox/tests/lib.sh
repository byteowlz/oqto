#!/usr/bin/env bash
# Shared helpers for oqto-sandbox hardening tests.
#
# All scenario scripts source this file. It MUST NOT run anything on source
# beyond defining functions and constants.

set -euo pipefail

# --- Paths ---------------------------------------------------------------------

TESTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="${TESTS_DIR}/fixtures"
REPO_ROOT="$(cd "${TESTS_DIR}/../../.." && pwd)"

# Deterministic per-run scratch dir; cleaned by trap in each scenario.
: "${OQTO_TEST_RUN_DIR:=${TMPDIR:-/tmp}/oqto-sandbox-tests.$$}"
export OQTO_TEST_RUN_DIR

# --- Binary discovery ----------------------------------------------------------
#
# Resolution order (first executable wins):
#   1. $OQTO_SANDBOX_BIN          (explicit override)
#   2. oqto-sandbox on PATH       (covers just-deploy / cargo install targets)
#   3. /var/lib/oqto/releases/current/bin/oqto-sandbox (deploy tree, if PATH missed it)
#   4. backend/target/release/oqto-sandbox (local build)
#   5. backend/target/debug/oqto-sandbox   (local debug build)
#
# Note: when you edit sandbox code locally and want to test your changes
# without re-deploying, set OQTO_SANDBOX_BIN="$PWD/backend/target/release/oqto-sandbox"
# so the PATH-resolved installed binary does not shadow the fresh build.

find_oqto_sandbox() {
  if [[ -n "${OQTO_SANDBOX_BIN:-}" ]]; then
    echo "${OQTO_SANDBOX_BIN}"
    return 0
  fi

  if command -v oqto-sandbox >/dev/null 2>&1; then
    command -v oqto-sandbox
    return 0
  fi

  local candidates=(
    "/var/lib/oqto/releases/current/bin/oqto-sandbox"
    "${REPO_ROOT}/backend/target/release/oqto-sandbox"
    "${REPO_ROOT}/backend/target/debug/oqto-sandbox"
  )
  for c in "${candidates[@]}"; do
    if [[ -x "${c}" ]]; then
      echo "${c}"
      return 0
    fi
  done

  echo "" # caller decides how to fail
  return 1
}

OQTO_SANDBOX_BIN_RESOLVED=""
require_oqto_sandbox() {
  if [[ -n "${OQTO_SANDBOX_BIN_RESOLVED}" ]]; then
    return 0
  fi
  OQTO_SANDBOX_BIN_RESOLVED="$(find_oqto_sandbox || true)"
  if [[ -z "${OQTO_SANDBOX_BIN_RESOLVED}" ]]; then
    fail "oqto-sandbox binary not found. Build with: cargo build --release -p oqto-sandbox"
  fi
}

run_sandbox() {
  # Usage: run_sandbox --config <toml> [--dry-run] [-v] -- <cmd...>
  require_oqto_sandbox
  "${OQTO_SANDBOX_BIN_RESOLVED}" "$@"
}

# --- Output --------------------------------------------------------------------

if [[ -t 1 ]]; then
  C_RED="$(printf '\033[0;31m')"
  C_GREEN="$(printf '\033[0;32m')"
  C_YELLOW="$(printf '\033[0;33m')"
  C_BLUE="$(printf '\033[0;34m')"
  C_BOLD="$(printf '\033[1m')"
  C_OFF="$(printf '\033[0m')"
else
  C_RED="" C_GREEN="" C_YELLOW="" C_BLUE="" C_BOLD="" C_OFF=""
fi

SCENARIO_NAME="${SCENARIO_NAME:-$(basename "${0:-lib.sh}" .sh)}"

info() { printf '%s[info]%s %s\n' "${C_BLUE}" "${C_OFF}" "$*"; }
warn() { printf '%s[warn]%s %s\n' "${C_YELLOW}" "${C_OFF}" "$*"; }
note() { printf '  %s\n' "$*"; }

ASSERT_PASSED=0
ASSERT_FAILED=0
ASSERT_SKIPPED=0

pass() {
  ASSERT_PASSED=$((ASSERT_PASSED + 1))
  printf '  %sPASS%s %s\n' "${C_GREEN}" "${C_OFF}" "$*"
}

fail() {
  ASSERT_FAILED=$((ASSERT_FAILED + 1))
  printf '  %sFAIL%s %s\n' "${C_RED}" "${C_OFF}" "$*"
}

skip() {
  ASSERT_SKIPPED=$((ASSERT_SKIPPED + 1))
  printf '  %sSKIP%s %s\n' "${C_YELLOW}" "${C_OFF}" "$*"
}

scenario_header() {
  printf '\n%s== %s ==%s\n' "${C_BOLD}" "${SCENARIO_NAME}" "${C_OFF}"
}

scenario_summary() {
  local status="OK"
  local colour="${C_GREEN}"
  if [[ "${ASSERT_FAILED}" -gt 0 ]]; then
    status="FAIL"
    colour="${C_RED}"
  fi
  printf '%s[%s]%s %s: %d passed, %d failed, %d skipped\n' \
    "${colour}" "${status}" "${C_OFF}" "${SCENARIO_NAME}" \
    "${ASSERT_PASSED}" "${ASSERT_FAILED}" "${ASSERT_SKIPPED}"

  if [[ "${ASSERT_FAILED}" -gt 0 ]]; then
    return 1
  fi
  return 0
}

# --- Assertions ----------------------------------------------------------------

assert_success() {
  # assert_success "<description>" -- <cmd...>
  local desc="$1"; shift
  [[ "$1" == "--" ]] && shift
  if "$@" >/dev/null 2>&1; then
    pass "${desc}"
  else
    fail "${desc} (command: $*)"
  fi
}

assert_failure() {
  # assert_failure "<description>" -- <cmd...>
  local desc="$1"; shift
  [[ "$1" == "--" ]] && shift
  if "$@" >/dev/null 2>&1; then
    fail "${desc} (command unexpectedly succeeded: $*)"
  else
    pass "${desc}"
  fi
}

assert_exit_code() {
  # assert_exit_code "<desc>" <expected> -- <cmd...>
  local desc="$1"; shift
  local expected="$1"; shift
  [[ "$1" == "--" ]] && shift
  local rc=0
  "$@" >/dev/null 2>&1 || rc=$?
  if [[ "${rc}" == "${expected}" ]]; then
    pass "${desc} (rc=${rc})"
  else
    fail "${desc} (expected rc=${expected}, got rc=${rc})"
  fi
}

assert_stdout_contains() {
  # assert_stdout_contains "<desc>" "<needle>" -- <cmd...>
  local desc="$1"; shift
  local needle="$1"; shift
  [[ "$1" == "--" ]] && shift
  local out
  out="$("$@" 2>&1 || true)"
  if [[ "${out}" == *"${needle}"* ]]; then
    pass "${desc}"
  else
    fail "${desc} (needle '${needle}' not found)"
    printf '    --- begin output ---\n%s\n    --- end output ---\n' "${out}" >&2
  fi
}

assert_stdout_not_contains() {
  local desc="$1"; shift
  local needle="$1"; shift
  [[ "$1" == "--" ]] && shift
  local out
  out="$("$@" 2>&1 || true)"
  if [[ "${out}" == *"${needle}"* ]]; then
    fail "${desc} (unwanted '${needle}' present)"
    printf '    --- begin output ---\n%s\n    --- end output ---\n' "${out}" >&2
  else
    pass "${desc}"
  fi
}

# --- Kernel capability probes --------------------------------------------------

have_landlock() {
  # ABI reported by landlock_create_ruleset(NULL, 0, VERSION=1).
  python3 - <<'PY' >/dev/null 2>&1
import ctypes, sys
try:
    rc = ctypes.CDLL(None).syscall(444, 0, 0, 1)
except Exception:
    sys.exit(1)
sys.exit(0 if rc >= 1 else 1)
PY
}

have_seccomp_bpf() {
  [[ -r "${OQTO_SECCOMP_BPF:-/etc/oqto/seccomp/default.bpf}" ]]
}

have_bwrap() { command -v bwrap >/dev/null 2>&1; }

have_user_namespaces() {
  local v
  v="$(cat /proc/sys/user/max_user_namespaces 2>/dev/null || echo 0)"
  [[ "${v}" -gt 0 ]]
}

# --- Workspace + config helpers -------------------------------------------------

make_workspace() {
  local dir="${OQTO_TEST_RUN_DIR}/ws-${1:-default}"
  if ! mkdir -p "${dir}" 2>/dev/null; then
    printf 'FATAL: make_workspace: mkdir %s failed (quota? perm?)\n' "${dir}" >&2
    exit 3
  fi
  echo "${dir}"
}

write_toml() {
  # write_toml <dest> <<'EOF' ... EOF
  #
  # Hard-exits the scenario on any write failure. Silent partial writes
  # (e.g. EDQUOT) cause oqto-sandbox to fall back to defaults and produce
  # misleading assertion failures, so we refuse to proceed.
  local dest="$1"
  if ! mkdir -p "$(dirname "${dest}")" 2>/dev/null; then
    printf 'FATAL: write_toml: mkdir %s failed (quota? perm?)\n' \
      "$(dirname "${dest}")" >&2
    exit 3
  fi
  if ! cat >"${dest}"; then
    printf 'FATAL: write_toml: write %s failed (quota? perm?)\n' "${dest}" >&2
    exit 3
  fi
  if [[ ! -s "${dest}" ]]; then
    printf 'FATAL: write_toml: %s is empty after write\n' "${dest}" >&2
    exit 3
  fi
}

# Fail if the run dir cannot absorb a real write. Two checks are needed:
#
#   1. `df` free space (covers full filesystem)
#   2. An actual write probe of size $2 KB (covers per-user quota, which df
#      does not reflect, and read-only mounts).
#
# Quota exhaustion on $TMPDIR is the #1 cause of cascading-failure sessions;
# df alone reports the fs as having gigabytes free while the user cannot
# write a single byte.
check_run_dir_writable() {
  local need_mb="${1:-20}"
  local probe_kb="${2:-1024}" # 1 MB probe by default
  local avail_kb
  avail_kb="$(df -Pk "${OQTO_TEST_RUN_DIR}" 2>/dev/null | awk 'NR==2 {print $4}')"
  if [[ -n "${avail_kb}" ]]; then
    local avail_mb=$((avail_kb / 1024))
    if [[ "${avail_mb}" -lt "${need_mb}" ]]; then
      printf 'FATAL: %s has only %dMB free per df (<%dMB required)\n' \
        "${OQTO_TEST_RUN_DIR}" "${avail_mb}" "${need_mb}" >&2
      printf '  hint: set OQTO_TEST_RUN_DIR=<dir-with-space> and retry\n' >&2
      return 1
    fi
  fi

  # Real write probe. Catches per-user quota (EDQUOT) that df misses.
  local probe="${OQTO_TEST_RUN_DIR}/.probe-$$"
  if ! dd if=/dev/zero of="${probe}" bs=1024 count="${probe_kb}" status=none 2>/dev/null; then
    rm -f "${probe}" 2>/dev/null
    printf 'FATAL: cannot write %dKB to %s (likely user quota)\n' \
      "${probe_kb}" "${OQTO_TEST_RUN_DIR}" >&2
    printf '  hint: set OQTO_TEST_RUN_DIR=<dir-with-space> and retry; check quota -s\n' >&2
    return 1
  fi
  local wrote
  wrote="$(stat -c '%s' "${probe}" 2>/dev/null || echo 0)"
  rm -f "${probe}" 2>/dev/null
  local need_bytes=$((probe_kb * 1024))
  if [[ "${wrote}" -lt "${need_bytes}" ]]; then
    printf 'FATAL: write probe truncated (%d < %d bytes) in %s\n' \
      "${wrote}" "${need_bytes}" "${OQTO_TEST_RUN_DIR}" >&2
    printf '  hint: likely user quota; set OQTO_TEST_RUN_DIR elsewhere\n' >&2
    return 1
  fi

  return 0
}

# Clean up run dir on EXIT unless OQTO_TEST_KEEP_DIR=1 is set.
cleanup_run_dir() {
  if [[ "${OQTO_TEST_KEEP_DIR:-0}" == "1" ]]; then
    warn "OQTO_TEST_KEEP_DIR=1; leaving ${OQTO_TEST_RUN_DIR} on disk"
    return 0
  fi
  rm -rf "${OQTO_TEST_RUN_DIR}" 2>/dev/null || true
}

mkdir -p "${OQTO_TEST_RUN_DIR}"
