#!/usr/bin/env bash
set -euo pipefail

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_SCHEMA_BACKEND="$SRC_DIR/backend/crates/oqto/examples/backend.config.schema.json"
SRC_SCHEMA_SANDBOX="$SRC_DIR/backend/crates/oqto/examples/sandbox.schema.json"
SRC_SCHEMA_INSTALL="$SRC_DIR/backend/crates/oqto/examples/oqto.install.schema.json"
DEST_DIR="$SRC_DIR/../../byteowlz/schemas/oqto"
DEST_SCHEMA_BACKEND="$DEST_DIR/oqto.backend.config.schema.json"
DEST_SCHEMA_SANDBOX="$DEST_DIR/oqto.sandbox.schema.json"
DEST_SCHEMA_INSTALL="$DEST_DIR/oqto.install.schema.json"

if [ ! -f "$SRC_SCHEMA_BACKEND" ]; then
  echo "Source schema not found: $SRC_SCHEMA_BACKEND" >&2
  exit 1
fi

mkdir -p "$DEST_DIR"
cp "$SRC_SCHEMA_BACKEND" "$DEST_SCHEMA_BACKEND"
cp "$SRC_SCHEMA_SANDBOX" "$DEST_SCHEMA_SANDBOX"
cp "$SRC_SCHEMA_INSTALL" "$DEST_SCHEMA_INSTALL"
echo "Copied schema to $DEST_SCHEMA_BACKEND"

cd "$DEST_DIR"
git pull
git add .
git commit -m "feat: updated oqto schema"
git push
echo "Committed and pushed schema changes"
