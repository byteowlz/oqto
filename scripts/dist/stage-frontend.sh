#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

BUILD_FIRST="${1:-}"
if [[ "$BUILD_FIRST" == "--build" ]]; then
  (cd frontend && bun run build)
fi

SRC="frontend/dist"
DST="dist/immutable/frontend"

if [[ ! -f "$SRC/index.html" ]]; then
  echo "error: missing built frontend at $SRC/index.html (run with --build or 'just build-frontend' first)" >&2
  exit 1
fi

mkdir -p "$(dirname "$DST")"
rsync -a --delete "$SRC/" "$DST/"

echo "frontend staging complete"
