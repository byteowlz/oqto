#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/dependencies.toml"

if ! command -v just >/dev/null 2>&1; then
  echo "warning: just is not installed; skipping dependencies.toml update" >&2
  exit 0
fi

( cd "$ROOT" && just update-deps )

"$ROOT/scripts/maint/update-deps-github.sh"

if git diff --quiet -- "$MANIFEST"; then
  exit 0
fi

git add "$MANIFEST"
