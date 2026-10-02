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

install_ext_for_home "$home" "$src"
[[ "$(cat "$home/.pi/agent/extensions/pi-history-search/index.ts")" == new ]]
[[ "$(cat "$home/.pi/agent/extensions/custom/index.ts")" == 'leave me alone' ]]
[[ ! -e "$home/.pi/agent/extensions/auto-rename" ]]
grep -F -- "-H -u $(id -un) -- cp -r" "$tmp/calls" >/dev/null

printf 'updated\n' > "$src/pi-history-search/index.ts"
install_ext_for_home "$home" "$src"
[[ "$(cat "$home/.pi/agent/extensions/pi-history-search/index.ts")" == updated ]]
[[ "$(cat "$home/.pi/agent/extensions/custom/index.ts")" == 'leave me alone' ]]

touch "$tmp/fail-copy"
if install_ext_for_home "$home" "$src"; then
  echo 'failed copy was silently accepted' >&2
  exit 1
fi
rm "$tmp/fail-copy"

rm "$src/pi-history-search/index.ts"
if install_ext_for_home "$home" "$src"; then
  echo 'missing required extension was silently accepted' >&2
  exit 1
fi
echo 'per-home Pi extension sync OK'
