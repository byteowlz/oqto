#!/usr/bin/env bash
# Rootless transport fixture: no SSH, sudo, services or real user profiles.
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
remote_dir="$(mktemp -d /tmp/oqto-extensions.XXXXXXXX)"
trap 'rm -rf -- "$remote_dir"' EXIT
DEPENDENCY_POLICY_FILE="$ROOT_DIR/dependencies.toml"
H_LOCAL=(false)
H_SSH=(fixture)
H_NAME=(fixture)
SYNC_EXTENSIONS=true
DRY_RUN=false
require_sudo_leases() { return 0; }
log() { :; }
ssh() { printf '%s\n' "$remote_dir"; }
scp() {
  [[ "$1" != -- ]] || shift
  local destination="${!#}"
  [[ "$destination" == "fixture:$remote_dir/" ]]
  while [[ $# -gt 1 ]]; do cp -- "$1" "$remote_dir/"; shift; done
}
host_exec_sudo() {
  [[ "$1" == false && "$2" == fixture ]]
  [[ "$3" == *'--skip-pi --non-interactive'* ]]
}
# Load the actual driver function without executing its main/authentication path.
source <(awk '/^sync_extensions_host\(\) \{/ {on=1} on {print} on && /^\}/ {exit}' "$ROOT_DIR/scripts/deploy.sh")
sync_extensions_host 0
for name in sync-agent-runtime.sh pi-default-extensions.txt managed-pi-extensions.py pi-extension-supersessions.json; do
  cmp -- "$ROOT_DIR/scripts/dist/$name" "$remote_dir/$name"
done
cmp -- "$DEPENDENCY_POLICY_FILE" "$remote_dir/dependencies.toml"
# The staged entrypoint must resolve its own helper and policy without the source
# checkout. Sourcing with both phases skipped performs no install or network IO.
bash -c '
  source "$1" --manifest "$2" --skip-pi --skip-extensions
  [[ -n "$EXT_REF" && -n "$PI_VERSION" ]]
  [[ -s "$SCRIPT_DIR/managed-pi-extensions.py" && -s "$PI_EXTENSION_POLICY_FILE" ]]
' _ "$remote_dir/sync-agent-runtime.sh" "$remote_dir/dependencies.toml"
printf 'complete remote extension staging OK\n'
