#!/usr/bin/env bash
# Read-only regression with synthetic hosts; no private deployment inventory.
set -euo pipefail
cd "$(dirname "$0")/../.."
tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT
log="$tmpdir/output"
config="$tmpdir/hosts.toml"
printf '%s\n' \
    '[[host]]' 'name = "personal-host"' 'ssh = ""' 'local = true' \
    'mode = "single-user"' 'user = "fixture"' \
    '[[host]]' 'name = "shared-host"' 'ssh = "example.invalid"' 'local = false' \
    'mode = "multi-user"' 'user = "fixture"' > "$config"
common=(--config "$config" --dry-run)
bash scripts/deploy.sh --dev --host personal-host "${common[@]}" >"$log" 2>&1
rg -q 'deploy-fast profile' "$log"
rg -q 'dev\.[0-9]{14}\.g[0-9a-f]{8}' "$log"
rg -q -- '--allow-dirty-source' "$log"
if bash scripts/deploy.sh --dev --host shared-host "${common[@]}" >"$log" 2>&1; then
    echo 'shared host wrongly accepted dev build' >&2; exit 1
fi
rg -q 'requires --allow-dev-shared' "$log"
bash scripts/deploy.sh --dev --allow-dev-shared --host shared-host "${common[@]}" >"$log" 2>&1
rg -q -- '--allow-dirty-source' "$log"
if bash scripts/deploy.sh --dev --artifact /tmp/not-an-artifact --host personal-host "${common[@]}" >"$log" 2>&1; then
    echo 'dev artifact combination wrongly accepted' >&2; exit 1
fi
rg -q 'do not combine with --artifact' "$log"
echo 'dev deploy tests passed'
