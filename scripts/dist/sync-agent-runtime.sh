#!/usr/bin/env bash
# Sync the agent runtime (pi + byteowlz pi-extensions) to the pinned versions,
# system-wide and for every platform user. Single source of truth used by both
# `setup` (initial install) and `deploy` (per-host refresh) — so pi/extensions
# stay current on deploy, not just at first setup.
#
# Idempotent. Pins come from dependencies.toml unless overridden. Runs on the
# target host; needs sudo for the system dirs and per-user installs.
#
# Usage: sync-agent-runtime.sh [--manifest PATH] [--pi-version V] [--ext-ref R]
#                              [--ext-repo URL] [--skip-pi] [--skip-extensions]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="${MANIFEST:-$SCRIPT_DIR/../../dependencies.toml}"
PI_VERSION=""
EXT_REF=""
EXT_REPO="https://github.com/byteowlz/pi-agent-extensions.git"
DO_PI=true
DO_EXT=true

# Extensions installed by default. oqto-usermgr embeds this same file for
# new-principal provisioning; this script handles setup/deploy backfills.
PI_DEFAULT_EXTENSIONS_FILE="${PI_DEFAULT_EXTENSIONS_FILE:-$SCRIPT_DIR/pi-default-extensions.txt}"
[[ -f "$PI_DEFAULT_EXTENSIONS_FILE" ]] || {
  echo "error: Pi default extension list missing: $PI_DEFAULT_EXTENSIONS_FILE" >&2
  exit 1
}
mapfile -t PI_DEFAULT_EXTENSIONS < <(grep -Ev '^[[:space:]]*(#|$)' "$PI_DEFAULT_EXTENSIONS_FILE")
[[ " ${PI_DEFAULT_EXTENSIONS[*]} " == *" pi-history-search "* ]] || {
  echo "error: canonical Pi extension list must include pi-history-search" >&2
  exit 1
}

# Superseded or fully removed extension dir names. Pruning them on every sync
# both clears stale duplicates from the octo->oqto and pre-`pi-` renames (a
# stale copy alongside its pi-* successor aborts Pi RPC startup with a fatal
# tool conflict -> zero models -> deploy verification failure) and retires
# extensions that have been dropped entirely. All are byteowlz-owned names,
# never user-custom, so pruning is safe.
# pi-openai-completions-convert-think-tags: removed -- MiniMax works on the
# built-in openai-completions api via reasoning_content; the think-tag adapter
# is no longer needed (see oqto-4brd).
PI_LEGACY_EXTENSIONS=(
  auto-rename azure-empty-response-guard introspection
  oqto-bridge octo-bridge oqto-todos octo-todos
  custom-context-files read-image-guard read-file-guard
  openai-completions-convert-think-tags
  pi-openai-completions-convert-think-tags
)

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest) MANIFEST="$2"; shift 2 ;;
    --pi-version) PI_VERSION="$2"; shift 2 ;;
    --ext-ref) EXT_REF="$2"; shift 2 ;;
    --ext-repo) EXT_REPO="$2"; shift 2 ;;
    --skip-pi) DO_PI=false; shift ;;
    --skip-extensions) DO_EXT=false; shift ;;
    --list-extensions) printf '%s\n' "${PI_DEFAULT_EXTENSIONS[@]}"; exit 0 ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

log() { printf '[agent-sync] %s\n' "$*"; }
err() { printf '[agent-sync] error: %s\n' "$*" >&2; }

# Read a value from dependencies.toml [external]: read_pin <key>
read_pin() {
  local key="$1"
  [[ -f "$MANIFEST" ]] || return 0
  sed -n '/^\[external\]/,/^\[/{s/^'"$key"' *= *"\([^"]*\)".*/\1/p}' "$MANIFEST" | head -1
}

[[ -n "$PI_VERSION" ]] || PI_VERSION="$(read_pin pi)"
[[ -n "$EXT_REF" ]] || EXT_REF="$(read_pin pi-extensions)"

# --- pi (official standalone runtime) ----------------------------------------
sync_pi() {
  local version="$1" checksum_key checksum runtime_script
  [[ -n "$version" && "$version" != "latest" ]] || {
    err "pi version not pinned (got '${version:-}')"
    return 1
  }

  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) checksum_key="pi-linux-x64-sha256" ;;
    Linux-aarch64|Linux-arm64) checksum_key="pi-linux-arm64-sha256" ;;
    *) err "unsupported Pi runtime platform: $(uname -s)-$(uname -m)"; return 1 ;;
  esac
  checksum="$(read_pin "$checksum_key")"
  [[ "$checksum" =~ ^[0-9a-f]{64}$ ]] || {
    err "missing or invalid $checksum_key pin in $MANIFEST"
    return 1
  }

  runtime_script="${PI_RUNTIME_SCRIPT:-$SCRIPT_DIR/pi-runtime.sh}"
  [[ -x "$runtime_script" ]] || {
    err "Pi runtime installer missing or not executable: $runtime_script"
    return 1
  }

  log "staging official Pi $version standalone runtime ($checksum_key)"
  "$runtime_script" --version "$version" --sha256 "$checksum" --install
}

# --- extensions (system + every user) ----------------------------------------
ext_cache() {
  local cache="${XDG_CACHE_HOME:-$HOME/.cache}/oqto/pi-agent-extensions"
  if [[ -d "$cache/.git" ]]; then
    git -C "$cache" remote set-url origin "$EXT_REPO" >/dev/null 2>&1 || true
    git -C "$cache" fetch --quiet --tags origin >/dev/null 2>&1 || true
  else
    rm -rf "$cache"; mkdir -p "$(dirname "$cache")"
    git clone --quiet "$EXT_REPO" "$cache" >/dev/null 2>&1 || { err "clone failed: $EXT_REPO"; return 1; }
  fi
  git -C "$cache" checkout --quiet --force "$EXT_REF" >/dev/null 2>&1 \
    || git -C "$cache" reset --hard --quiet "$EXT_REF" >/dev/null 2>&1 \
    || { err "cannot check out extensions ref '$EXT_REF'"; return 1; }
  echo "$cache"
}

# Install into a home as its owner; only /etc/skel is installed as root.
# The source must be the world-readable system staging tree, not the invoking
# user's private git cache. Args: <home> <src>
install_ext_for_home() {
  local home="$1" src="$2"
  local dir="$home/.pi/agent/extensions" owner group parent legacy ext
  owner="$(stat -c '%U' "$home")" || return 1
  group="$(stat -c '%G' "$home")" || return 1

  as_owner() {
    if [[ "$owner" == root ]]; then
      sudo -- "$@"
    else
      sudo -H -u "$owner" -- "$@"
    fi
  }

  # Repair only deployment-created parent directories, never arbitrary user
  # config. A previous interrupted root install may have left them root-owned.
  if [[ "$owner" != root ]]; then
    for parent in "$home/.pi" "$home/.pi/agent" "$dir"; do
      if [[ -d "$parent" && "$(stat -c '%U' "$parent")" == root ]]; then
        sudo chown "$owner:$group" "$parent" || return 1
      fi
    done
  fi
  as_owner mkdir -p "$dir" || return 1

  # Only deployment-owned names may be repaired/pruned; custom extensions and
  # every other file under ~/.pi remain untouched.
  for legacy in "${PI_LEGACY_EXTENSIONS[@]}"; do
    [[ -e "$dir/$legacy" || -L "$dir/$legacy" ]] || continue
    if [[ "$owner" != root && ! -L "$dir/$legacy" ]]; then
      sudo chown -hR "$owner:$group" "$dir/$legacy" || return 1
    fi
    as_owner rm -rf -- "$dir/$legacy" || return 1
    log "  pruned legacy extension: $dir/$legacy"
  done
  for ext in "${PI_DEFAULT_EXTENSIONS[@]}"; do
    [[ -f "$src/$ext/index.ts" ]] || { err "extension missing from source: $ext"; return 1; }
    if [[ -e "$dir/$ext" || -L "$dir/$ext" ]]; then
      if [[ "$owner" != root && ! -L "$dir/$ext" ]]; then
        sudo chown -hR "$owner:$group" "$dir/$ext" || return 1
      fi
      as_owner rm -rf -- "$dir/$ext" || return 1
    fi
    as_owner cp -r -- "$src/$ext" "$dir/$ext" || return 1
    as_owner rm -f -- "$dir/$ext/install.sh" || return 1
    [[ "$(stat -c '%U' "$dir/$ext")" == "$owner" ]] || {
      err "extension not owned by $owner after install: $dir/$ext"
      return 1
    }
  done
}

sync_extensions() {
  [[ -n "$EXT_REF" ]] || { err "pi-extensions ref not pinned"; return 1; }
  local src; src="$(ext_cache)" || return 1
  log "extensions ref: $(git -C "$src" rev-parse --short HEAD 2>/dev/null)"

  # Fail-closed: required extensions must exist at this ref.
  local ext missing=0
  for ext in "${PI_DEFAULT_EXTENSIONS[@]}"; do
    [[ -f "$src/$ext/index.ts" ]] || { err "required extension missing at ref: $ext"; missing=1; }
  done
  [[ "$missing" -eq 0 ]] || return 1

  log "syncing extensions to /usr/share/oqto/pi-agent-extensions (new-user source)"
  sudo mkdir -p /usr/share/oqto/pi-agent-extensions || return 1
  sudo rsync -a --delete --exclude='.git' "$src/" /usr/share/oqto/pi-agent-extensions/ || return 1

  # Every existing platform user + the invoking user + /etc/skel (new users).
  local home count=0
  for home in /home/oqto_* "$HOME" /etc/skel; do
    [[ -d "$home" ]] || continue
    install_ext_for_home "$home" /usr/share/oqto/pi-agent-extensions || return 1
    count=$((count + 1))
    log "  extensions -> $home"
  done
  log "extensions synced to $count location(s)"
}

verify_managed_pi_selection() {
  local config="$HOME/.config/oqto/config.toml"
  [[ -f "$config" ]] || return 0
  local configured
  configured="$(sed -n '/^\[pi\]/,/^\[/{s/^executable *= *"\([^"]*\)".*/\1/p}' "$config" | head -1)"
  case "$configured" in
    */.bun/bin/pi|*/.local/bin/pi)
      err "stale Pi override shadows the managed runtime: $configured"
      err "back up $config and set [pi].executable = \"pi\""
      return 1
      ;;
  esac
}

sync_agent_runtime_main() {
  log "manifest=$MANIFEST  pi=${PI_VERSION:-<unset>}  pi-extensions=${EXT_REF:-<unset>}"
  local rc=0
  $DO_PI && { sync_pi "$PI_VERSION" || rc=1; }
  $DO_PI && { verify_managed_pi_selection || rc=1; }
  $DO_EXT && { sync_extensions || rc=1; }
  [[ "$rc" -eq 0 ]] && log "agent runtime sync complete" || err "agent runtime sync had failures"
  return "$rc"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  sync_agent_runtime_main
fi
