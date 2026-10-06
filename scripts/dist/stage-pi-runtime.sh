#!/usr/bin/env bash
# Canonical Linux full-release runtime: exact official npm Pi + pinned Node.
# This only stages into the owning build worktree; never promotes a live link.
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
python3 scripts/dist/verify-worktree-ownership.py --root "$ROOT_DIR"
[[ $# == 0 ]] || { echo 'Usage: stage-pi-runtime.sh' >&2; exit 2; }
mkdir -p dist/immutable/pi-runtime
exec scripts/dist/stage-pi-node-runtime.sh "$ROOT_DIR/dist/immutable/pi-runtime/pi"
