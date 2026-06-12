#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

# Guard against system-wide cargo homes (e.g. /usr/local/cargo) that are not
# writable for normal users in CI/dev shells.
if [[ -z "${CARGO_HOME:-}" || "${CARGO_HOME:-}" == /usr/local/cargo* || ! -w "${CARGO_HOME:-$HOME/.cargo}" ]]; then
  export CARGO_HOME="$HOME/.cargo"
fi
if [[ -z "${RUSTUP_HOME:-}" || "${RUSTUP_HOME:-}" == /usr/local/rustup* || ! -w "${RUSTUP_HOME:-$HOME/.rustup}" ]]; then
  export RUSTUP_HOME="$HOME/.rustup"
fi
mkdir -p "$CARGO_HOME" "$RUSTUP_HOME"

cargo_build() {
  echo "[dist-stage] CARGO_HOME=$CARGO_HOME RUSTUP_HOME=$RUSTUP_HOME cargo build $*"
  CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" cargo build "$@"
}

BUILD_FIRST="${1:-}"
if [[ "$BUILD_FIRST" == "--build" ]]; then
  cd backend
  cargo_build --release -p oqto --bin oqto --bin oqto-sandbox --bin pi-bridge
  cargo_build --release -p oqtoctl --bin oqtoctl
  cargo_build --release -p oqto-setup --bin oqto-setup
  cargo_build --release -p oqto-runner --bin oqto-runner
  cargo_build --release -p oqto-files --bin oqto-files
  cargo_build --release -p oqto-usermgr --bin oqto-usermgr
  cd "$ROOT_DIR"
fi

mkdir -p dist/immutable/bin
for bin in oqto oqtoctl oqto-setup oqto-runner oqto-files oqto-sandbox oqto-usermgr pi-bridge; do
  src="backend/target/release/$bin"
  dst="dist/immutable/bin/$bin"
  if [[ ! -x "$src" ]]; then
    echo "error: missing built binary $src (run with --build or build first)" >&2
    exit 1
  fi
  cp "$src" "$dst"
  chmod 0755 "$dst"
done

echo "binary staging complete"
