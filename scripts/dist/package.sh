#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
python3 scripts/dist/verify-worktree-ownership.py --root "$ROOT_DIR"

# Fail closed: never produce a tarball missing manifest-declared content.
# Strict (no --allow-missing-*): binaries must be staged and pi extensions /
# templates must be synced onto disk (scripts/dist/sync.sh) before packaging.
python3 scripts/lint/verify-dist-manifest.py

VERSION="${1:-$(date +%Y%m%d%H%M%S)}"
TARGET="${2:-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)}"
OUT_DIR="${OUT_DIR:-dist/out}"
NAME="oqto-${VERSION}-${TARGET}"
STAGE_DIR="$OUT_DIR/$NAME"

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"

cp -R dist/immutable "$STAGE_DIR/"
# The tracked directory placeholder is not a binary and must never become
# /usr/local/bin/.gitkeep when oqto-setup relinks staged entrypoints.
rm -f "$STAGE_DIR/immutable/bin/.gitkeep"
cp -R dist/mutable-templates "$STAGE_DIR/"
cp dist/manifest.toml "$STAGE_DIR/manifest.toml"

# .gitkeep exists only so git tracks otherwise-empty directories. Shipping it
# leaks a repository artifact into provisioned user content (agent skills and
# prompts). Drop the markers; tar preserves the directories themselves.
find "$STAGE_DIR" -type f -name '.gitkeep' -delete

# Use the actual typed activation contract before producing a bundle. This is
# read-only and does not run services, Pi, SQLite, or activation.
"$STAGE_DIR/immutable/bin/oqto-setup" check-release --release-dir "$STAGE_DIR"

TARBALL="$OUT_DIR/${NAME}.tar.gz"
mkdir -p "$OUT_DIR"

# Avoid AppleDouble metadata on macOS while retaining archive leak validation.
COPYFILE_DISABLE=1 tar -C "$OUT_DIR" -czf "$TARBALL" "$NAME"

# Read the whole listing: `grep -q` would exit early, and the resulting
# SIGPIPE plus `pipefail` would report a clean tarball no matter what leaked.
leaked="$(tar tzf "$TARBALL" | grep '/\.gitkeep$' || true)"
if [ -n "$leaked" ]; then
  echo "error: repository placeholder files leaked into $TARBALL:" >&2
  echo "$leaked" >&2
  exit 1
fi

sha256sum "$TARBALL" > "$TARBALL.sha256"

echo "packaged: $TARBALL"
echo "checksum: $TARBALL.sha256"
