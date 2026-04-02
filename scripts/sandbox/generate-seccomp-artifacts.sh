#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
POLICY="$ROOT_DIR/backend/crates/oqto/examples/seccomp/default.policy.toml"
OUT_DIR="$ROOT_DIR/backend/crates/oqto/examples/seccomp"

cd "$ROOT_DIR/backend"
cargo run -p oqto-sandbox --bin seccomp-policy-gen -- \
  --policy "$POLICY" \
  --out-dir "$OUT_DIR" \
  --arches "x86_64,aarch64"

echo "Generated seccomp artifacts in $OUT_DIR"
