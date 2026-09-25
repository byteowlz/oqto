#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
sync="$root/scripts/dist/sync.sh"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT

"$sync" --print-refs > "$scratch/refs"
expected_templates="$(awk -F '"' '/^oqto-templates =/{print $2;exit}' "$root/dependencies.toml")"
expected_extensions="$(awk -F '"' '/^pi-extensions =/{print $2;exit}' "$root/dependencies.toml")"
grep -Fxq "oqto-templates=$expected_templates" "$scratch/refs"
grep -Fxq "pi-agent-extensions=$expected_extensions" "$scratch/refs"
if PI_AGENT_EXTENSIONS_REF=main "$sync" --print-refs > "$scratch/output" 2>&1; then
  echo 'moving extension ref override was accepted' >&2
  exit 1
fi
grep -Fq 'source ref overrides must match dependencies.toml' "$scratch/output"

mkdir "$scratch/unpinned"
git -C "$scratch/unpinned" init -q
git -C "$scratch/unpinned" -c user.name=fixture -c user.email=fixture@example.test \
  commit -q --allow-empty -m 'unpinned source'
if OQTO_TEMPLATES_REPO="$scratch/unpinned" "$sync" > "$scratch/output" 2>&1; then
  echo 'local unpinned source checkout was accepted' >&2
  exit 1
fi
grep -Eq 'pinned oqto-templates commit .* unavailable|does not match pinned commit' "$scratch/output"

mkdir -p "$scratch/cache/oqto-templates"
printf 'user cache data\n' > "$scratch/cache/oqto-templates/keep"
if OQTO_TEMPLATES_REPO="$scratch/absent" OQTO_DIST_CACHE_DIR="$scratch/cache" \
  "$sync" > "$scratch/output" 2>&1; then
  echo 'unknown cache content was overwritten' >&2
  exit 1
fi
grep -Fq 'refusing to overwrite unknown oqto-templates cache' "$scratch/output"
grep -Fxq 'user cache data' "$scratch/cache/oqto-templates/keep"

echo 'dist sources are pinned; dirty/local caches are preserved'
