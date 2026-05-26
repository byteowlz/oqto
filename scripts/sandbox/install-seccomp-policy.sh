#!/usr/bin/env bash
set -euo pipefail

# Install precompiled seccomp-bpf artifact for current architecture.
#
# Expected source artifacts (shipped/deployed separately):
#   backend/crates/oqto/examples/seccomp/default-x86_64.bpf
#   backend/crates/oqto/examples/seccomp/default-aarch64.bpf
#
# Installs to:
#   /usr/local/share/oqto/seccomp/default.bpf

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info() { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[OK]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

if [[ $EUID -ne 0 ]]; then
  error "Run as root (sudo)."
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARCH="$(uname -m)"

case "$ARCH" in
  x86_64|amd64)
    SRC="$SCRIPT_DIR/backend/crates/oqto/examples/seccomp/default-x86_64.bpf"
    ;;
  aarch64|arm64)
    SRC="$SCRIPT_DIR/backend/crates/oqto/examples/seccomp/default-aarch64.bpf"
    ;;
  *)
    error "Unsupported architecture: $ARCH"
    error "Provide custom artifact and install manually."
    exit 1
    ;;
esac

if [[ ! -f "$SRC" ]]; then
  error "Missing precompiled artifact: $SRC"
  warn "Generate artifact from policy source:"
  warn "  backend/crates/oqto/examples/seccomp/default.policy.toml"
  exit 1
fi

# Policy files are not secrets; keep them world-readable so local-user
# sandboxes can open them without privileged group membership.
TARGET_DIR="/usr/local/share/oqto/seccomp"
TARGET_FILE="${TARGET_DIR}/default.bpf"
LEGACY_DIR="/etc/oqto/seccomp"
LEGACY_FILE="${LEGACY_DIR}/default.bpf"

install -d -m 755 -o root -g root "${TARGET_DIR}"
install -m 644 -o root -g root "$SRC" "${TARGET_FILE}"

# Backward compatibility: keep legacy path as symlink for existing configs.
install -d -m 755 -o root -g root "${LEGACY_DIR}"
ln -sfn "${TARGET_FILE}" "${LEGACY_FILE}"

success "Installed seccomp policy: ${TARGET_FILE} (owner root:root, mode 644)"
info "Created legacy symlink: ${LEGACY_FILE} -> ${TARGET_FILE}"
info "Set in sandbox config: seccomp_mode='enforce' + seccomp_bpf_path='${TARGET_FILE}'"
