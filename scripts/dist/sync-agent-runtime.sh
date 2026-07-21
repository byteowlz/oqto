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

# Extensions installed by default (mirrors scripts/setup/06-pi-extensions.sh).
PI_DEFAULT_EXTENSIONS=(
  pi-auto-rename pi-azure-empty-response-guard pi-introspection pi-oqto-bridge
  pi-oqto-todos pi-custom-context-files pi-read-image-guard pi-read-file-guard
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

resolve_bun() {
  command -v bun 2>/dev/null && return 0
  [[ -x "$HOME/.bun/bin/bun" ]] && { echo "$HOME/.bun/bin/bun"; return 0; }
  [[ -x /usr/local/bin/bun ]] && { echo /usr/local/bin/bun; return 0; }
  return 1
}

# --- pi (system-wide) ---------------------------------------------------------
verify_pi_packages() {
  local root="$1" expected="$2" label="$3" pkg actual
  for pkg in pi-coding-agent pi-agent-core pi-ai pi-tui; do
    actual="$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "$root/@earendil-works/$pkg/package.json" | head -1)"
    [[ "$actual" == "$expected" ]] || {
      err "$label package mismatch: $pkg expected=$expected actual=${actual:-missing}"
      return 1
    }
  done
}

verify_pi_rpc() {
  local bin="$1" expected="$2" label="$3"
  local version output errors
  version="$($bin --version 2>/dev/null | head -1)"
  [[ "$version" == "$expected" ]] || {
    err "$label version check failed: expected=$expected actual=${version:-unavailable}"
    return 1
  }

  output="$(mktemp)"; errors="$(mktemp)"
  if ! printf '%s\n' '{"id":"agent-sync-smoke","type":"get_available_models"}' \
      | timeout 30 "$bin" --mode rpc --no-session >"$output" 2>"$errors"; then
    err "$label RPC process failed: $(tail -3 "$errors" | tr '\n' ' ')"
    rm -f "$output" "$errors"
    return 1
  fi
  if ! python3 - "$output" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    try:
        row = json.loads(line)
    except json.JSONDecodeError:
        continue
    if (row.get("type") == "response" and row.get("id") == "agent-sync-smoke"
            and row.get("success") is True
            and isinstance((row.get("data") or {}).get("models"), list)
            and len((row.get("data") or {}).get("models")) > 0):
        raise SystemExit(0)
raise SystemExit(1)
PY
  then
    err "$label RPC model-discovery smoke test returned no successful response"
    tail -3 "$errors" >&2 || true
    rm -f "$output" "$errors"
    return 1
  fi
  rm -f "$output" "$errors"
  log "$label verified: pi $version + RPC model discovery"
}

sync_pi() {
  local version="$1"
  [[ -n "$version" && "$version" != "latest" ]] || { err "pi version not pinned (got '${version:-}')"; return 1; }
  local bun; bun="$(resolve_bun)" || { err "bun not found; cannot install pi"; return 1; }

  # Pin the whole directly-coupled Pi package family. Installing only the CLI
  # leaves its caret dependencies free to reuse a newer hoisted pi-ai, which
  # can remove exports and make every Pi process fail during module loading.
  log "installing coherent pi package set @ ${version}"
  "$bun" install -g \
    "@earendil-works/pi-coding-agent@${version}" \
    "@earendil-works/pi-agent-core@${version}" \
    "@earendil-works/pi-ai@${version}" \
    "@earendil-works/pi-tui@${version}"

  local global_root="$HOME/.bun/install/global/node_modules"
  local src="$global_root/@earendil-works/pi-coding-agent"
  local sys="/usr/local/lib/pi-coding-agent"
  [[ -d "$src" ]] || { err "pi global install not found at $src"; return 1; }
  verify_pi_packages "$global_root" "$version" "user-global pi" || return 1
  verify_pi_rpc "$HOME/.bun/bin/pi" "$version" "user-global pi" || return 1

  log "syncing pi to $sys (system-wide)"
  sudo rm -rf "$sys"
  sudo cp -a "$src" "$sys"
  sudo chmod -R a+rX "$sys"
  ( cd "$sys" && sudo /usr/local/bin/bun install --frozen-lockfile 2>/dev/null \
      || sudo /usr/local/bin/bun install 2>/dev/null )
  # Override caret-resolved direct Pi dependencies with the exact coherent set.
  ( cd "$sys" && sudo /usr/local/bin/bun add --exact \
      "@earendil-works/pi-agent-core@${version}" \
      "@earendil-works/pi-ai@${version}" \
      "@earendil-works/pi-tui@${version}" >/dev/null )

  # Self-links so user extensions importing the host package resolve (see 05).
  sudo mkdir -p "$sys/node_modules/@earendil-works" "$sys/node_modules/@mariozechner"
  sudo ln -sfn "$sys" "$sys/node_modules/@earendil-works/pi-coding-agent"
  sudo ln -sfn "$sys" "$sys/node_modules/@mariozechner/pi-coding-agent"

  sudo tee /usr/local/bin/pi >/dev/null <<'PIEOF'
#!/usr/bin/env bash
PI_PKG="/usr/local/lib/pi-coding-agent"
if [ ! -f "$PI_PKG/dist/cli.js" ]; then
  echo "Error: pi-coding-agent not found at $PI_PKG" >&2
  exit 1
fi
BUN="${HOME}/.bun/bin/bun"
[ -x "$BUN" ] || BUN="/usr/local/bin/bun"
[ -x "$BUN" ] || { echo "Error: bun not found" >&2; exit 1; }
export PI_PACKAGE_DIR="$PI_PKG"
export NODE_PATH="$PI_PKG/node_modules${NODE_PATH:+:$NODE_PATH}"
exec "$BUN" "$PI_PKG/dist/cli.js" "$@"
PIEOF
  sudo chmod 755 /usr/local/bin/pi
  verify_pi_packages "$sys/node_modules" "$version" "system pi" || return 1
  verify_pi_rpc /usr/local/bin/pi "$version" "system pi" || return 1
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

# Install the default extensions into <home>/.pi/agent/extensions, owned by the
# home's owner. Args: <home> <src>
install_ext_for_home() {
  local home="$1" src="$2"
  local dir="$home/.pi/agent/extensions"
  local owner; owner="$(stat -c '%U:%G' "$home" 2>/dev/null || echo 'root:root')"
  sudo mkdir -p "$dir"
  local ext
  for ext in "${PI_DEFAULT_EXTENSIONS[@]}"; do
    [[ -f "$src/$ext/index.ts" ]] || { err "extension missing from source: $ext"; continue; }
    sudo rm -rf "$dir/$ext"
    sudo cp -r "$src/$ext" "$dir/$ext"
    sudo rm -f "$dir/$ext/install.sh"
  done
  sudo chown -R "$owner" "$home/.pi" 2>/dev/null || true
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
  sudo mkdir -p /usr/share/oqto/pi-agent-extensions
  sudo rsync -a --delete --exclude='.git' "$src/" /usr/share/oqto/pi-agent-extensions/

  # Every existing platform user + the invoking user + /etc/skel (new users).
  local home count=0
  for home in /home/oqto_* "$HOME" /etc/skel; do
    [[ -d "$home" ]] || continue
    install_ext_for_home "$home" "$src"
    count=$((count + 1))
    log "  extensions -> $home"
  done
  log "extensions synced to $count location(s)"
}

log "manifest=$MANIFEST  pi=${PI_VERSION:-<unset>}  pi-extensions=${EXT_REF:-<unset>}"
rc=0
$DO_PI && { sync_pi "$PI_VERSION" || rc=1; }
$DO_EXT && { sync_extensions || rc=1; }
[[ "$rc" -eq 0 ]] && log "agent runtime sync complete" || err "agent runtime sync had failures"
exit "$rc"
