#!/usr/bin/env bash
set -euo pipefail

# Live seccomp canary: run a harmless non-interactive Pi task under oqto-sandbox
# with seccomp_mode=enforce and verify expected behavior.
#
# Defaults:
#   MODEL=zgx/qwen3.6-35b
#   WORKDIR=/var/tmp/oqto-seccomp-canary
#   BPF_PATH=/usr/local/share/oqto/seccomp/default.bpf
#   SANDBOX_BIN=backend/target/debug/oqto-sandbox (fallback: oqto-sandbox on PATH)
#
# Usage:
#   scripts/e2e/seccomp-live-canary.sh
#   MODEL=zgx/qwen3.6-35b WORKDIR=/var/tmp/my-canary scripts/e2e/seccomp-live-canary.sh

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MODEL="${MODEL:-zgx/qwen3.6-35b}"
WORKDIR="${WORKDIR:-/var/tmp/oqto-seccomp-canary}"
BPF_PATH="${BPF_PATH:-/usr/local/share/oqto/seccomp/default.bpf}"
CANARY_CONFIG="${CANARY_CONFIG:-/var/tmp/seccomp-enforce-canary.toml}"
LOG_FILE="${LOG_FILE:-/var/tmp/seccomp-live-canary.log}"

if [[ -x "${ROOT_DIR}/backend/target/debug/oqto-sandbox" ]]; then
  SANDBOX_BIN="${SANDBOX_BIN:-${ROOT_DIR}/backend/target/debug/oqto-sandbox}"
else
  SANDBOX_BIN="${SANDBOX_BIN:-oqto-sandbox}"
fi

red='\033[0;31m'
green='\033[0;32m'
blue='\033[0;34m'
yellow='\033[0;33m'
nc='\033[0m'

info() { echo -e "${blue}[seccomp-canary]${nc} $*"; }
pass() { echo -e "${green}[PASS]${nc} $*"; }
warn() { echo -e "${yellow}[WARN]${nc} $*"; }
fail() { echo -e "${red}[FAIL]${nc} $*"; }

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail "Missing required command: $1"
    exit 1
  fi
}

require_cmd pi
if [[ "${SANDBOX_BIN}" == "oqto-sandbox" ]]; then
  require_cmd oqto-sandbox
fi

if [[ ! -r "${BPF_PATH}" ]]; then
  fail "BPF policy not readable: ${BPF_PATH}"
  echo "Install with: sudo scripts/sandbox/install-seccomp-policy.sh"
  exit 1
fi

mkdir -p "${WORKDIR}"
mkdir -p "${WORKDIR}/.oqto"

cat >"${CANARY_CONFIG}" <<EOF
enabled = true
profile = "canary"

[profiles.canary]
deny_read = ["~/.ssh", "~/.gnupg", "~/.aws"]
allow_write = ["/tmp", "${WORKDIR}", "~/.pi", "~/.config/oqto", "~/.cache", "~/.local/share"]
deny_write = ["~/.config/oqto/sandbox.toml"]
isolate_network = false
isolate_pid = true
drop_all_caps = false
disable_userns = true
assert_userns_disabled = false
no_new_privs = true
seccomp_mode = "enforce"
seccomp_bpf_path = "${BPF_PATH}"
landlock_mode = "off"
overlay_enabled = false
overlay_root = "~/.oqto/overlays"
overlay_paths = []
EOF

stamp="$(date +%Y%m%d-%H%M%S)"
smoke_file="smoke-${stamp}.txt"
prompt="Create a file named ${smoke_file} containing exactly: seccomp enforce canary ok. Then print current directory and list files in current dir only."

info "Running canary in ${WORKDIR}"
info "Model: ${MODEL}"
info "Sandbox: ${SANDBOX_BIN}"

set +e
(
  cd "${WORKDIR}"
  "${SANDBOX_BIN}" -v \
    --config "${CANARY_CONFIG}" \
    --workspace "${WORKDIR}" \
    -- pi -p --no-session --no-extensions --no-skills --model "${MODEL}" "${prompt}"
) 2>&1 | tee "${LOG_FILE}"
rc=${PIPESTATUS[0]}
set -e

if [[ ${rc} -ne 0 ]]; then
  fail "Canary command failed (rc=${rc})"
  exit ${rc}
fi

if ! rg -q "Seccomp enabled via bwrap fd" "${LOG_FILE}"; then
  fail "Did not observe seccomp wiring log in output"
  exit 1
fi

if [[ ! -f "${WORKDIR}/${smoke_file}" ]]; then
  fail "Expected smoke file not created: ${WORKDIR}/${smoke_file}"
  exit 1
fi

content="$(cat "${WORKDIR}/${smoke_file}")"
normalized="$(printf '%s' "${content}" | tr '[:upper:]' '[:lower:]' | sed -E 's/[[:space:]]+/ /g; s/[[:space:]]+$//; s/[[:punct:]]+$//')"
if [[ "${normalized}" != "seccomp enforce canary ok" ]]; then
  fail "Unexpected smoke file content: '${content}'"
  exit 1
fi

pass "Seccomp enforce live canary succeeded"
info "Smoke file: ${WORKDIR}/${smoke_file}"
info "Log file: ${LOG_FILE}"
warn "Config left at ${CANARY_CONFIG} for reuse"
