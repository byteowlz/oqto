#!/usr/bin/env bash
# Repeated release builds keep a pinned staged runtime and refuse a drifted one.
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT
mkdir -p "$tmp/scripts/dist" "$tmp/dist/immutable/pi-runtime/pi"
cp "$ROOT_DIR/scripts/dist/stage-pi-runtime.sh" "$ROOT_DIR/scripts/dist/verify-worktree-ownership.py" "$tmp/scripts/dist/"
cp "$ROOT_DIR/dist/manifest.toml" "$tmp/dist/"
# Never reached for an existing runtime; fails loudly if it is.
printf '#!/bin/sh\necho "stager must not run" >&2; exit 9\n' > "$tmp/scripts/dist/stage-pi-node-runtime.sh"
chmod +x "$tmp/scripts/dist/"*.sh
python3 - "$tmp/dist/manifest.toml" "$tmp/dist/immutable/pi-runtime/pi/oqto-runtime.json" <<'PY'
import json, sys, tomllib
release = tomllib.load(open(sys.argv[1], "rb"))["release"]
json.dump({"kind": "node-npm", "version": release["pi_version"], "node_version": release["node_version"],
           "node_archive_sha256": release["node_archive_sha256"], "pi_npm_integrity": release["pi_npm_integrity"],
           "package_lock_sha256": release["pi_package_lock_sha256"]}, open(sys.argv[2], "w"))
PY
"$tmp/scripts/dist/stage-pi-runtime.sh" | grep -q 'already staged at the pinned closure'
sed -i 's/"version": "[^"]*"/"version": "0.0.1"/' "$tmp/dist/immutable/pi-runtime/pi/oqto-runtime.json"
if "$tmp/scripts/dist/stage-pi-runtime.sh" > "$tmp/out" 2>&1; then
  echo 'drifted staged runtime was accepted' >&2; exit 1
fi
grep -q 'does not match the pinned closure' "$tmp/out"
rm "$tmp/dist/immutable/pi-runtime/pi/oqto-runtime.json"
if "$tmp/scripts/dist/stage-pi-runtime.sh" > "$tmp/out" 2>&1; then
  echo 'receipt-less staged runtime was accepted' >&2; exit 1
fi
grep -q 'no readable receipt' "$tmp/out"
echo 'stage-pi-runtime idempotence tests passed'
