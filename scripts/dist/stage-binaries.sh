#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

# Guard against system-wide cargo homes (e.g. /usr/local/cargo) that are not
# writable for normal users in CI/dev shells. RUSTUP_HOME may intentionally be
# system-wide/read-only; use an explicit toolchain instead of requiring a
# mutable `rustup default`.
if [[ -z "${CARGO_HOME:-}" || "${CARGO_HOME:-}" == /usr/local/cargo* || ! -w "${CARGO_HOME:-$HOME/.cargo}" ]]; then
  export CARGO_HOME="$HOME/.cargo"
fi
mkdir -p "$CARGO_HOME"

CARGO_TOOLCHAIN="${CARGO_TOOLCHAIN:-stable}"
CARGO_CMD=(cargo "+$CARGO_TOOLCHAIN")
if ! "${CARGO_CMD[@]}" --version >/dev/null 2>&1; then
  CARGO_CMD=(cargo)
fi

# glibc floor (ADR-0021): build linux-gnu binaries against an old glibc via
# cargo-zigbuild so the oqto platform runs on older hosts (Ubuntu 22.04/RHEL8+),
# not just on the (bleeding-edge) build host. Without this, a binary linked on
# e.g. Arch's glibc 2.43 fails on Ubuntu 24.04 with "GLIBC_2.43 not found".
# Falls back to a plain (non-portable) build if the zig toolchain is absent
# unless DIST_REQUIRE_GLIBC_FLOOR=1 forces it.
GLIBC_FLOOR="${GLIBC_FLOOR:-2.28}"
HOST_ARCH="$(uname -m)"
TARGET_TRIPLE="${HOST_ARCH}-unknown-linux-gnu"
USE_ZIGBUILD=false
if command -v cargo-zigbuild >/dev/null 2>&1 && command -v zig >/dev/null 2>&1; then
  USE_ZIGBUILD=true
elif [[ "${DIST_REQUIRE_GLIBC_FLOOR:-}" == "1" ]]; then
  echo "error: DIST_REQUIRE_GLIBC_FLOOR=1 but cargo-zigbuild/zig not found" >&2
  echo "  install: cargo install cargo-zigbuild --locked; and install zig; rustup target add $TARGET_TRIPLE" >&2
  exit 1
else
  echo "[dist-stage] WARNING: cargo-zigbuild/zig not found -> building WITHOUT a glibc floor." >&2
  echo "[dist-stage] WARNING: binaries may fail on hosts with older glibc than this build host." >&2
fi

# Release dir the built binaries land in (zigbuild uses the target subdir).
if $USE_ZIGBUILD; then
  REL_DIR="backend/target/${TARGET_TRIPLE}/release"
else
  REL_DIR="backend/target/release"
fi

cargo_build() {
  if $USE_ZIGBUILD; then
    echo "[dist-stage] ${CARGO_CMD[*]} zigbuild --target ${TARGET_TRIPLE}.${GLIBC_FLOOR} $*"
    CARGO_HOME="$CARGO_HOME" "${CARGO_CMD[@]}" zigbuild --target "${TARGET_TRIPLE}.${GLIBC_FLOOR}" "$@"
  else
    echo "[dist-stage] CARGO_HOME=$CARGO_HOME RUSTUP_HOME=${RUSTUP_HOME:-<unset>} ${CARGO_CMD[*]} build $*"
    CARGO_HOME="$CARGO_HOME" "${CARGO_CMD[@]}" build "$@"
  fi
}

BUILD_FIRST="${1:-}"
if [[ "$BUILD_FIRST" == "--build" ]]; then
  cd backend
  cargo_build --release -p oqto --bin oqto --bin oqto-sandbox --bin oqto-ssh-proxy --bin pi-bridge
  cargo_build --release -p oqtoctl --bin oqtoctl
  cargo_build --release -p oqto-setup --bin oqto-setup
  cargo_build --release -p oqto-runner --bin oqto-runner
  cargo_build --release -p oqto-files --bin oqto-files
  cargo_build --release -p oqto-usermgr --bin oqto-usermgr
  cd "$ROOT_DIR"
fi

mkdir -p dist/immutable/bin
for bin in oqto oqtoctl oqto-setup oqto-runner oqto-files oqto-sandbox oqto-ssh-proxy oqto-usermgr pi-bridge; do
  src="$REL_DIR/$bin"
  dst="dist/immutable/bin/$bin"
  if [[ ! -x "$src" ]]; then
    echo "error: missing built binary $src (run with --build or build first)" >&2
    exit 1
  fi
  cp "$src" "$dst"
  chmod 0755 "$dst"
done

echo "binary staging complete"
