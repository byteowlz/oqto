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

cargo_build() {
  echo "[dist-stage] CARGO_HOME=$CARGO_HOME RUSTUP_HOME=${RUSTUP_HOME:-<unset>} ${CARGO_CMD[*]} build $*"
  CARGO_HOME="$CARGO_HOME" "${CARGO_CMD[@]}" build "$@"
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
