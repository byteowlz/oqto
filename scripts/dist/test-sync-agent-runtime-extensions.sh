#!/usr/bin/env bash
# Isolated per-home installation regression; never invokes privilege escalation.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
printf 'pi-history-search\n' > "$tmp/defaults"
export PI_DEFAULT_EXTENSIONS_FILE="$tmp/defaults"
# shellcheck source=scripts/dist/sync-agent-runtime.sh
source "$SCRIPT_DIR/sync-agent-runtime.sh"

home="$tmp/home"
src="$tmp/source"
mkdir -p "$home/.pi/agent/extensions/custom" "$home/.pi/agent/extensions/auto-rename" "$src/pi-history-search"
printf 'leave me alone\n' > "$home/.pi/agent/extensions/custom/index.ts"
printf 'old\n' > "$home/.pi/agent/extensions/auto-rename/index.ts"
printf 'new\n' > "$src/pi-history-search/index.ts"
export PI_EXTENSION_POLICY_FILE="$tmp/policy.json"
python3 - "$PI_EXTENSION_POLICY_FILE" <<'PY'
import hashlib,json,sys
h=lambda s:hashlib.sha256(s.encode()).hexdigest()
json.dump({'version':1,'supersessions':{'auto-rename':{'replacement':None,'index_sha256':[h('old\n')]}},'managed_index_sha256':{'pi-history-search':[h('new\n'),h('updated\n')]}},open(sys.argv[1],'w'))
PY

# Test double: record the requested user identity and execute as this test user.
sudo() {
  printf '%s\n' "$*" >> "$tmp/calls"
  if [[ "$NON_INTERACTIVE" == true ]]; then
    [[ "${1:-}" == -n ]] || { echo 'interactive sudo attempted' >&2; return 1; }
  fi
  [[ "${1:-}" != -n ]] || shift
  if [[ "${1:-}" == -H ]]; then
    shift
    [[ "${1:-}" == -u ]] || return 1
    [[ "$2" == "$(id -un)" ]] || return 1
    shift 2
  fi
  [[ "${1:-}" != -- ]] || shift
  "$@"
}

backups() { find "$home/.pi/agent/extension-backups" -mindepth 2 -maxdepth 2 2>/dev/null | sort; }
export SYNC_STAMP=20261009T000000Z
install_ext_for_home "$home" "$src"
[[ "$(cat "$home/.pi/agent/extensions/pi-history-search/index.ts")" == new ]]
[[ "$(cat "$home/.pi/agent/extensions/custom/index.ts")" == 'leave me alone' ]]
# The retired managed extension is preserved, never deleted.
[[ ! -e "$home/.pi/agent/extensions/auto-rename" ]]
legacy_backup="$(find "$home/.pi/agent/extension-backups" -path '*/auto-rename/index.ts')"
[[ "$(cat "$legacy_backup")" == old ]]
grep -F -- "-H -u $(id -un) -- python3 -I -c" "$tmp/calls" >/dev/null
! grep -F -- "$SCRIPT_DIR/managed-pi-extensions.py" "$tmp/calls" >/dev/null || { echo 'principal was asked to traverse operator checkout' >&2; exit 1; }
! grep -F -- ' chown ' "$tmp/calls" >/dev/null || { echo 'unexpected privileged ownership repair' >&2; exit 1; }

# Identical content is a no-op: no copy, no backup.
: > "$tmp/calls"
before="$(backups)"
install_ext_for_home "$home" "$src"
[[ "$(backups)" == "$before" ]]

# A user-modified managed extension is backed up before replacement.
printf 'user tweak\n' > "$home/.pi/agent/extensions/pi-history-search/local.ts"
printf 'updated\n' > "$src/pi-history-search/index.ts"
install_ext_for_home "$home" "$src"
[[ "$(cat "$home/.pi/agent/extensions/pi-history-search/index.ts")" == updated ]]
[[ ! -e "$home/.pi/agent/extensions/pi-history-search/local.ts" ]]
modified_backup="$(find "$home/.pi/agent/extension-backups" -path '*/pi-history-search/local.ts')"
[[ "$(cat "$modified_backup")" == 'user tweak' ]]
[[ "$(cat "$home/.pi/agent/extensions/custom/index.ts")" == 'leave me alone' ]]

# A failed copy leaves the installed extension exactly as it was.
printf 'next\n' > "$src/pi-history-search/index.ts"
mkfifo "$src/pi-history-search/cannot-copy-fifo"
if install_ext_for_home "$home" "$src"; then
  echo 'failed copy was silently accepted' >&2
  exit 1
fi
rm "$src/pi-history-search/cannot-copy-fifo"
[[ "$(cat "$home/.pi/agent/extensions/pi-history-search/index.ts")" == updated ]]
[[ -z "$(find "$home/.pi/agent" -maxdepth 1 -name '.extension-stage-*')" ]]

rm "$src/pi-history-search/index.ts"
if install_ext_for_home "$home" "$src"; then
  echo 'missing required extension was silently accepted' >&2
  exit 1
fi
[[ "$(cat "$home/.pi/agent/extensions/pi-history-search/index.ts")" == updated ]]
echo 'per-home Pi extension sync OK'
