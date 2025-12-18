#!/usr/bin/env bash
set -euo pipefail

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_SCHEMA_BACKEND="$SRC_DIR/backend/examples/workspace-backend-config.schema.json"
SRC_SCHEMA_FILESERVER="$SRC_DIR/fileserver/examples/fileserver-config.schema.json"
DEST_DIR="$SRC_DIR/../../byteowlz/schemas/octo"
DEST_SCHEMA_BACKEND="$DEST_DIR/octo.backend.config.schema.json"
DEST_SCHEMA_FILESERVER="$DEST_DIR/octo.fileserver.config.schema.json"

if [ ! -f "$SRC_SCHEMA_BACKEND" ]; then
  echo "Source schema not found: $SRC_SCHEMA_FILESERVER" >&2
  exit 1
fi

mkdir -p "$DEST_DIR"
cp "$SRC_SCHEMA_BACKEND" "$DEST_SCHEMA_BACKEND"
cp "$SRC_SCHEMA_FILESERVER" "$DEST_SCHEMA_FILESERVER"
echo "Copied schema to $DEST_SCHEMA_BACKEND"

cd "$DEST_DIR"
git pull
git add .
git commit -m "feat: updated octo schema"
git push
echo "Committed and pushed schema changes"
