#!/usr/bin/env bash
# Reproduce the Oct20 producer omission without root, real homes or credentials.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export PI_DEFAULT_EXTENSIONS_FILE="$tmp/defaults"
printf 'pi-history-search\npi-todolist\n' > "$PI_DEFAULT_EXTENSIONS_FILE"
source "$here/sync-agent-runtime.sh"
sudo() {
  [[ "${1:-}" != -n ]] || shift
  if [[ "${1:-}" == -H ]]; then shift 3; fi
  [[ "${1:-}" != -- ]] || shift
  "$@"
}
home="$tmp/home"; src="$tmp/source"
mkdir -p "$home/.pi/agent/extensions/pi-oqto-todos" "$home/.pi/agent/tasks" "$src/pi-history-search" "$src/pi-todolist"
printf 'obsolete managed Todo\n' > "$home/.pi/agent/extensions/pi-oqto-todos/index.ts"
printf 'task data must survive\n' > "$home/.pi/agent/tasks/state.json"
printf 'history\n' > "$src/pi-history-search/index.ts"
printf 'current Todo\n' > "$src/pi-todolist/index.ts"
export PI_EXTENSION_POLICY_FILE="$tmp/policy.json"
python3 - "$PI_EXTENSION_POLICY_FILE" <<'PY'
import hashlib,json,sys
h=lambda s:hashlib.sha256(s.encode()).hexdigest()
json.dump({'version':1,'supersessions':{'pi-oqto-todos':{'replacement':'pi-todolist','index_sha256':[h('obsolete managed Todo\n')]}},'managed_index_sha256':{}},open(sys.argv[1],'w'))
PY
install_ext_for_home "$home" "$src"
[[ ! -e "$home/.pi/agent/extensions/pi-oqto-todos" ]] || { echo 'REGRESSION: obsolete pi-oqto-todos remains beside pi-todolist' >&2; exit 1; }
[[ "$(< "$home/.pi/agent/tasks/state.json")" == 'task data must survive' ]]
[[ -f "$home/.pi/agent/extensions/pi-todolist/index.ts" ]]
echo 'Todo producer supersession OK'
