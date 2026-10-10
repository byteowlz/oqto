#!/usr/bin/env bash
# Canonical Linux full-release runtime: exact official npm Pi + pinned Node.
# This only stages into the owning build worktree; never promotes a live link.
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
python3 scripts/dist/verify-worktree-ownership.py --root "$ROOT_DIR"
[[ $# == 0 ]] || { echo 'Usage: stage-pi-runtime.sh' >&2; exit 2; }
mkdir -p dist/immutable/pi-runtime
staged="$ROOT_DIR/dist/immutable/pi-runtime/pi"
# Idempotent for repeated release builds: keep an already staged runtime only
# if its receipt records exactly the pinned closure. Anything else is refused,
# never replaced; package.sh then fully validates it via `check-release`.
if [[ -e "$staged" || -L "$staged" ]]; then
  python3 - "$staged/oqto-runtime.json" dist/manifest.toml <<'PY'
import json, sys, tomllib
receipt_path, manifest_path = sys.argv[1:]
try:
    receipt = json.load(open(receipt_path))
except (OSError, ValueError):
    sys.exit(f"error: staged Pi runtime has no readable receipt: {receipt_path}")
release = tomllib.load(open(manifest_path, "rb"))["release"]
expected = {
    "version": release["pi_version"],
    "node_version": release["node_version"],
    "node_archive_sha256": release["node_archive_sha256"],
    "pi_npm_integrity": release["pi_npm_integrity"],
    "package_lock_sha256": release["pi_package_lock_sha256"],
}
drift = {key: (receipt.get(key), value) for key, value in expected.items() if receipt.get(key) != value}
if drift:
    sys.exit(
        "error: staged Pi runtime does not match the pinned closure "
        f"{drift}; remove dist/immutable/pi-runtime/pi deliberately to restage"
    )
print(f"Pi runtime {expected['version']} already staged at the pinned closure")
PY
  exit
fi
exec scripts/dist/stage-pi-node-runtime.sh "$staged"
