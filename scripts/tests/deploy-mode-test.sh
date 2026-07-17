#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEPLOY="$ROOT_DIR/scripts/deploy.sh"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

cat >"$tmpdir/hosts.toml" <<'EOF'
[[host]]
name = "test-local"
ssh = ""
local = true
mode = "single-user"
user = "test"
frontend = false
web_root = ""
binaries = []
services = []
EOF

common=(
  --host test-local
  --config "$tmpdir/hosts.toml"
  --dry-run
  --skip-build
  --allow-legacy-path
  --status
)

release_output="$($DEPLOY "${common[@]}" --mode release 2>&1)"
grep -q 'dependency gate: eavs >=' <<<"$release_output"
if grep -q 'installed version preserved' <<<"$release_output"; then
  echo "release mode unexpectedly used development dependency policy" >&2
  exit 1
fi

dev_output="$($DEPLOY "${common[@]}" --mode dev 2>&1)"
grep -q 'Development dependency mode: preserving target-installed tools' <<<"$dev_output"
grep -q 'dependency presence gate: eavs (installed version preserved)' <<<"$dev_output"
if grep -q 'dependency gate: eavs >=' <<<"$dev_output"; then
  echo "dev mode unexpectedly enforced the release version pin" >&2
  exit 1
fi

if "$DEPLOY" --mode unsupported --config "$tmpdir/hosts.toml" >/dev/null 2>"$tmpdir/error"; then
  echo "unknown deploy mode unexpectedly succeeded" >&2
  exit 1
fi
grep -q "Invalid --mode 'unsupported'" "$tmpdir/error"

help_output="$($DEPLOY --help)"
grep -q -- '--mode MODE' <<<"$help_output"

echo "deploy mode tests passed"
