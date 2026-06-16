#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/dependencies.toml"

if ! command -v curl >/dev/null 2>&1; then
  echo "warning: curl not available; skipping GitHub release checks" >&2
  exit 0
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "warning: jq not available; skipping GitHub release checks" >&2
  exit 0
fi

echo "Checking GitHub releases for binary availability..."

in_section=false
while IFS= read -r line; do
  if [[ "$line" =~ ^\[byteowlz\]$ ]]; then
    in_section=true
    continue
  elif [[ "$line" =~ ^\[.+\]$ ]]; then
    in_section=false
    continue
  fi

  $in_section || continue
  [[ "$line" =~ ^[a-zA-Z] ]] || continue

  key=$(echo "$line" | sed 's/ *=.*//')
  old_val=$(echo "$line" | sed 's/.*= *"\([^"]*\)".*/\1/')

  repo="$key"
  api_url="https://api.github.com/repos/byteowlz/${repo}/releases/latest"

  release_json=$(curl -fsSL "$api_url" 2>/dev/null || true)
  if [[ -z "$release_json" ]]; then
    echo "  $key: no GitHub release found"
    continue
  fi

  tag=$(echo "$release_json" | jq -r '.tag_name // empty')
  assets_count=$(echo "$release_json" | jq -r '.assets | length')

  if [[ -z "$tag" || "$assets_count" == "0" ]]; then
    echo "  $key: latest release has no binary assets"
    continue
  fi

  version="${tag#v}"
  if [[ -z "$version" ]]; then
    continue
  fi

  sed -i "s/^${key} = \"[^\"]*\"/${key} = \"${version}\"/" "$MANIFEST"
  if [[ "$version" != "$old_val" ]]; then
    echo "  $key: $old_val -> $version"
  else
    echo "  $key: $version (unchanged)"
  fi
done < "$MANIFEST"

