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

# Exercise actual manifest-driven defaults, not only explicit CLI overrides.
printf '%s\n' '[external]' 'pi = "1.4.0" # pinned' \
  '  pi-extensions = "0123456789abcdef0123456789abcdef01234567"' > "$tmp/pins.toml"
parsed="$(MANIFEST="$tmp/pins.toml" bash -c \
  'source "$1" --skip-pi --skip-extensions; printf "%s\\n%s" "$PI_VERSION" "$EXT_REF"' \
  _ "$SCRIPT_DIR/sync-agent-runtime.sh")"
[[ "$parsed" == $'1.4.0\n0123456789abcdef0123456789abcdef01234567' ]]
printf '%s\n' '[external]' > "$tmp/pins.toml"
missing="$(MANIFEST="$tmp/pins.toml" bash -c \
  'source "$1" --skip-pi --skip-extensions; printf "%s%s" "$PI_VERSION" "$EXT_REF"' \
  _ "$SCRIPT_DIR/sync-agent-runtime.sh")"
[[ -z "$missing" ]]
for invalid in 'pi = 42' 'pi = "unterminated'; do
  printf '%s\n' '[external]' "$invalid" > "$tmp/pins.toml"
  if MANIFEST="$tmp/pins.toml" bash -c \
    'source "$1" --skip-pi --skip-extensions' _ "$SCRIPT_DIR/sync-agent-runtime.sh" \
    > "$tmp/pin-error" 2>&1; then
    echo 'invalid runtime pin was accepted' >&2; exit 1
  fi
done

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

# A private root download cache must not make the shared source unreadable.
mode_source="$tmp/private-source"
mode_stage="$tmp/shared-source"
mkdir -p "$mode_source/module"
printf 'public module\n' > "$mode_source/module/index.ts"
printf '#!/usr/bin/env bash\nexit 0\n' > "$mode_source/module/helper"
chmod 0700 "$mode_source" "$mode_source/module" "$mode_source/module/helper"
chmod 0600 "$mode_source/module/index.ts"
printf 'private sentinel\n' > "$tmp/private-target"
chmod 0600 "$tmp/private-target"
ln -s "$tmp/private-target" "$mode_source/module/external-link"
(umask 077; stage_ext_source "$mode_source" "$mode_stage")
python3 - "$mode_source" "$mode_stage" <<'PY'
from pathlib import Path
import stat,sys
source,stage=map(Path,sys.argv[1:])
assert stat.S_IMODE(source.stat().st_mode)==0o700
assert stat.S_IMODE((source/'module/index.ts').stat().st_mode)==0o600
assert stat.S_IMODE((stage.parent/'private-target').stat().st_mode)==0o600
for path in (stage,*stage.rglob('*')):
    if path.is_symlink(): continue
    mode=stat.S_IMODE(path.stat().st_mode)
    assert mode & 0o044 == 0o044, (path,oct(mode))
    assert not mode & 0o022, (path,oct(mode))
    if path.is_dir(): assert mode & 0o011 == 0o011, (path,oct(mode))
assert (stage/'module/helper').stat().st_mode & 0o011 == 0o011
PY
ln -s "$mode_stage" "$tmp/shared-source-link"
if stage_ext_source "$mode_source" "$tmp/shared-source-link"; then
  echo 'symlinked shared destination was accepted' >&2; exit 1
fi

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
