#!/usr/bin/env bash
# Read-only regression: --dry-run never builds or activates.
set -euo pipefail
cd "$(dirname "$0")/../.."
log=$(mktemp)
trap 'rm -f "$log"' EXIT
bash scripts/deploy.sh --dev --host archvm --dry-run >"$log" 2>&1
grep -q 'deploy-fast profile' "$log"
grep -Eq 'dev\.[0-9]{14}\.g[0-9a-f]{8}' "$log"
grep -q -- '--allow-dirty-source' "$log"
if bash scripts/deploy.sh --dev --host octo-azure --dry-run >"$log" 2>&1; then
    echo 'shared host wrongly accepted dev build' >&2; exit 1
fi
grep -q 'requires --allow-dev-shared' "$log"
bash scripts/deploy.sh --dev --allow-dev-shared --host octo-azure --dry-run >"$log" 2>&1
grep -q -- '--allow-dirty-source' "$log"
if bash scripts/deploy.sh --dev --artifact /tmp/not-an-artifact --host archvm --dry-run >"$log" 2>&1; then
    echo 'dev artifact combination wrongly accepted' >&2; exit 1
fi
grep -q 'do not combine with --artifact' "$log"
echo 'dev deploy tests passed'
