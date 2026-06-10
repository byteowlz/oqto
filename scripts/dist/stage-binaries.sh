#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

BUILD_FIRST="${1:-}"
if [[ "$BUILD_FIRST" == "--build" ]]; then
  cd backend
  cargo build --release -p oqto --bin oqto --bin oqto-sandbox --bin pi-bridge
  cargo build --release -p oqtoctl --bin oqtoctl
  cargo build --release -p oqto-setup --bin oqto-setup
  cargo build --release -p oqto-runner --bin oqto-runner
  cargo build --release -p oqto-files --bin oqto-files
  cargo build --release -p oqto-usermgr --bin oqto-usermgr
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
