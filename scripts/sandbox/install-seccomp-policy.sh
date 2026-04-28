#!/usr/bin/env bash
set -euo pipefail

# Install precompiled seccomp-bpf artifact for current architecture.
#
# Expected source artifacts (shipped/deployed separately):
#   backend/crates/oqto/examples/seccomp/default-x86_64.bpf
#   backend/crates/oqto/examples/seccomp/default-aarch64.bpf
#
# Installs to:
#   /etc/oqto/seccomp/default.bpf

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

# Prefer the `oqto` system group (created by `just deploy`); fall back to
# root:root on single-user dev hosts where that group does not exist.
if getent group oqto >/dev/null 2>&1; then
  GROUP=oqto
  DIR_MODE=750
  FILE_MODE=640
else
  warn "group 'oqto' not found; installing as root:root (dev-host fallback)"
  GROUP=root
  DIR_MODE=755
  FILE_MODE=644
fi

install -d -m "$DIR_MODE" -o root -g "$GROUP" /etc/oqto/seccomp
install -m "$FILE_MODE" -o root -g "$GROUP" "$SRC" /etc/oqto/seccomp/default.bpf

success "Installed seccomp policy: /etc/oqto/seccomp/default.bpf (owner root:$GROUP, mode $FILE_MODE)"
info "Set in sandbox config: seccomp_mode='enforce' + seccomp_bpf_path='/etc/oqto/seccomp/default.bpf'"
