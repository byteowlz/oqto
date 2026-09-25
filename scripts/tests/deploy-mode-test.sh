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

printf 'not a release\n' > "$tmpdir/unverified.tar.gz"
if "$DEPLOY" "${common[@]}" --mode release --artifact "$tmpdir/unverified.tar.gz" >"$tmpdir/unverified.out" 2>&1; then
  echo "deploy accepted an artifact without independent checksum input" >&2
  exit 1
fi
grep -q 'checksum' "$tmpdir/unverified.out"
sha256sum "$tmpdir/unverified.tar.gz" > "$tmpdir/unverified.tar.gz.sha256"
verified_output="$("$DEPLOY" "${common[@]}" --mode release \
  --artifact "$tmpdir/unverified.tar.gz" --checksum "$tmpdir/unverified.tar.gz.sha256" 2>&1)"
grep -q 'dependency gate: eavs >=' <<<"$verified_output"

help_output="$($DEPLOY --help)"
grep -q -- '--mode MODE' <<<"$help_output"

# The release bootstrap may never regress to an elevated wildcard tar
# extraction or a host PATH fallback when an artifact is available.
python3 - "$DEPLOY" <<'PY'
from pathlib import Path
import sys
text = Path(sys.argv[1]).read_text()
for name in ("acquire_managed_tools", "deploy_via_oqto_setup_install"):
    body = text.split(f"\n{name}() {{\n", 1)[1].split("\n}\n", 1)[0]
    assert "stage_private_remote_release" in body, name
    assert "verified-setup-bootstrap.py" in body, name
    assert "tar -xzf" not in body and "--wildcards" not in body, name
PY

echo "deploy mode tests passed"
