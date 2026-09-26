#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

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

TARBALL="$OUT_DIR/${NAME}.tar.gz"
mkdir -p "$OUT_DIR"

# macOS bsdtar otherwise emits AppleDouble ._ metadata members outside the
# declared release root. The verified extractor must reject those members.
COPYFILE_DISABLE=1 tar -C "$OUT_DIR" -czf "$TARBALL" "$NAME"
sha256sum "$TARBALL" > "$TARBALL.sha256"

echo "packaged: $TARBALL"
echo "checksum: $TARBALL.sha256"
