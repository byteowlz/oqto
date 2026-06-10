#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

VERSION="${1:-$(date +%Y%m%d%H%M%S)}"
TARGET="${2:-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)}"
OUT_DIR="${OUT_DIR:-dist/out}"
NAME="oqto-${VERSION}-${TARGET}"
STAGE_DIR="$OUT_DIR/$NAME"

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"

cp -R dist/immutable "$STAGE_DIR/"
cp -R dist/mutable-templates "$STAGE_DIR/"
cp dist/manifest.toml "$STAGE_DIR/manifest.toml"

TARBALL="$OUT_DIR/${NAME}.tar.gz"
mkdir -p "$OUT_DIR"

tar -C "$OUT_DIR" -czf "$TARBALL" "$NAME"
sha256sum "$TARBALL" > "$TARBALL.sha256"

echo "packaged: $TARBALL"
echo "checksum: $TARBALL.sha256"
