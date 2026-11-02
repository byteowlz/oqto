#!/usr/bin/env bash
# Build-owned extension staging must match the selected closure on repeat builds.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
mkdir -p "$tmp/build/scripts/dist" "$tmp/build/dist" "$tmp/build/tools/provider-login"
cp "$root/scripts/dist/"{sync.sh,verify-worktree-ownership.py} "$tmp/build/scripts/dist/"
for file in worker.mjs broker.mjs pi-runtime.mjs README.md; do
  printf 'fixture\n' > "$tmp/build/tools/provider-login/$file"
done
mkdir -p "$tmp/templates/"{pi-agent,workdir-templates/default,onboarding-templates/onboarding} "$tmp/extensions/current"
printf 'fixture\n' > "$tmp/templates/pi-agent/AGENTS.md"
printf '{}\n' > "$tmp/templates/pi-agent/settings.json"
printf 'current\n' > "$tmp/extensions/current/index.ts"
for repo in templates extensions; do
  git -C "$tmp/$repo" init -q
  git -C "$tmp/$repo" add .
  git -C "$tmp/$repo" -c user.name=fixture -c user.email=fixture@example.test commit -qm fixture
done
printf '[external]\noqto-templates = "%s"\npi-extensions = "%s"\n' \
  "$(git -C "$tmp/templates" rev-parse HEAD)" "$(git -C "$tmp/extensions" rev-parse HEAD)" > "$tmp/build/dependencies.toml"
printf '[pi_agent_extensions]\nitems = ["current"]\n' > "$tmp/build/dist/manifest.toml"
output="$tmp/build/dist/immutable/defaults/pi-agent/extensions"
mkdir -p "$output/retired" "$output/current"
printf 'stale\n' > "$output/retired/index.ts"
printf 'stale extra\n' > "$output/current/removed.ts"
OQTO_TEMPLATES_REPO="$tmp/templates" PI_AGENT_EXTENSIONS_REPO="$tmp/extensions" "$tmp/build/scripts/dist/sync.sh" > "$tmp/log" 2>&1
[[ -f "$output/current/index.ts" ]]
[[ ! -e "$output/retired" ]] || { echo 'retired extension leaked into release staging' >&2; exit 1; }
[[ ! -e "$output/current/removed.ts" ]]
echo 'dist extension closure is exact; stale extensions do not survive repeat staging'
