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

# Public builds never require a private source by default. An explicit developer
# override remains supported; --print-refs performs no fetch or staging.
env -u PI_AGENT_EXTENSIONS_URL -u PI_AGENT_EXTENSIONS_PUBLIC_URL \
  bash -x "$sync" --print-refs > "$scratch/refs" 2> "$scratch/default-source"
grep -Fq 'PI_AGENT_EXTENSIONS_URL=https://github.com/byteowlz/pi-agent-extensions.git' "$scratch/default-source"
PI_AGENT_EXTENSIONS_URL=file:///fixture/private-extensions \
  bash -x "$sync" --print-refs > "$scratch/refs" 2> "$scratch/explicit-source"
grep -Fq 'PI_AGENT_EXTENSIONS_URL=file:///fixture/private-extensions' "$scratch/explicit-source"

# A sibling checkout that is not the clean pinned commit is never used: sync
# falls back to the pinned cache (here an unknown cache, so it stops before any
# network access) and leaves the checkout untouched.
mkdir "$scratch/unpinned"
git -C "$scratch/unpinned" init -q
git -C "$scratch/unpinned" -c user.name=fixture -c user.email=fixture@example.test \
  commit -q --allow-empty -m 'unpinned source'
printf 'work in progress\n' > "$scratch/unpinned/wip"
mkdir -p "$scratch/fallback-cache/oqto-templates"
printf 'user cache data\n' > "$scratch/fallback-cache/oqto-templates/keep"
if OQTO_TEMPLATES_REPO="$scratch/unpinned" OQTO_DIST_CACHE_DIR="$scratch/fallback-cache" \
  "$sync" > "$scratch/output" 2>&1; then
  echo 'local unpinned source checkout was accepted' >&2
  exit 1
fi
grep -Fq 'is not the clean pinned commit' "$scratch/output"
grep -Fq 'refusing to overwrite unknown oqto-templates cache' "$scratch/output"
grep -Fxq 'work in progress' "$scratch/unpinned/wip"

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
