#!/usr/bin/env bash
# Isolated per-home installation regression; never invokes privilege escalation.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
printf 'pi-history-search\n' > "$tmp/defaults"
export PI_DEFAULT_EXTENSIONS_FILE="$tmp/defaults"
# shellcheck source=sync-agent-runtime.sh
source "$SCRIPT_DIR/sync-agent-runtime.sh"

home="$tmp/home"
src="$tmp/source"
mkdir -p "$home/.pi/agent/extensions/custom" "$home/.pi/agent/extensions/auto-rename" "$src/pi-history-search"
printf 'leave me alone\n' > "$home/.pi/agent/extensions/custom/index.ts"
printf 'old\n' > "$home/.pi/agent/extensions/auto-rename/index.ts"
printf 'new\n' > "$src/pi-history-search/index.ts"

# Test double: record the requested user identity and execute as this test user.
sudo() {
  printf '%s\n' "$*" >> "$tmp/calls"
  if [[ -f "$tmp/fail-copy" && " $* " == *' cp -r '* ]]; then
    return 42
  fi
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
[[ "$(cat "$home/.pi/agent/extension-backups/$SYNC_STAMP/auto-rename/index.ts")" == old ]]
grep -F -- "-H -u $(id -un) -- cp -r" "$tmp/calls" >/dev/null

# Identical content is a no-op: no copy, no backup.
: > "$tmp/calls"
before="$(backups)"
install_ext_for_home "$home" "$src"
! grep -F -- 'cp -r' "$tmp/calls" >/dev/null || { echo 'unchanged extension was recopied' >&2; exit 1; }
[[ "$(backups)" == "$before" ]]

# A user-modified managed extension is backed up before replacement.
printf 'user tweak\n' > "$home/.pi/agent/extensions/pi-history-search/local.ts"
printf 'updated\n' > "$src/pi-history-search/index.ts"
install_ext_for_home "$home" "$src"
[[ "$(cat "$home/.pi/agent/extensions/pi-history-search/index.ts")" == updated ]]
[[ ! -e "$home/.pi/agent/extensions/pi-history-search/local.ts" ]]
[[ "$(cat "$home/.pi/agent/extension-backups/$SYNC_STAMP/pi-history-search/local.ts")" == 'user tweak' ]]
[[ "$(cat "$home/.pi/agent/extensions/custom/index.ts")" == 'leave me alone' ]]

# A failed copy leaves the installed extension exactly as it was.
printf 'next\n' > "$src/pi-history-search/index.ts"
touch "$tmp/fail-copy"
if install_ext_for_home "$home" "$src"; then
  echo 'failed copy was silently accepted' >&2
  exit 1
fi
rm "$tmp/fail-copy"
[[ "$(cat "$home/.pi/agent/extensions/pi-history-search/index.ts")" == updated ]]
[[ -z "$(find "$home/.pi/agent/extensions" -maxdepth 1 -name '.oqto-staging-*')" ]]

rm "$src/pi-history-search/index.ts"
if install_ext_for_home "$home" "$src"; then
  echo 'missing required extension was silently accepted' >&2
  exit 1
fi
[[ "$(cat "$home/.pi/agent/extensions/pi-history-search/index.ts")" == updated ]]
echo 'per-home Pi extension sync OK'
