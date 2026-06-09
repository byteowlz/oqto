#!/usr/bin/env bash
set -euo pipefail

# Migrate legacy mmry SQLite stores into workspace-local .mmry/mmry.jsonl files.
# Works for single-user and multi-user hosts.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY_MIGRATE="${MMRY_MIGRATE_SCRIPT:-$SCRIPT_DIR/migrate_legacy_mmry_to_jsonl.py}"
MODE="auto"   # auto|single|multi
QUIET="false"

usage() {
  cat <<'USAGE'
Usage: migrate-mmry-jsonl.sh [--mode auto|single|multi] [--quiet]

Behavior:
- single mode: migrate current $HOME workspaces under $HOME/oqto/*
- multi mode:  migrate all /home/oqto_* workspaces under /home/<user>/oqto/*
- auto mode:   run both (deduplicated)

Only migrates when target .mmry/mmry.jsonl does not already exist with content.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode) MODE="${2:-}"; shift 2 ;;
    --quiet) QUIET="true"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done

log() {
  [[ "$QUIET" == "true" ]] && return 0
  echo "[mmry-migrate] $*"
}

if [[ ! -f "$PY_MIGRATE" ]]; then
  log "migration script not found: $PY_MIGRATE (skipping)"
  exit 0
fi

parse_remote_repo_name() {
  local ws="$1"
  local remote url name
  remote=$(git -C "$ws" config --get remote.origin.url 2>/dev/null || true)
  [[ -z "$remote" ]] && return 0
  url="${remote##*/}"
  name="${url%.git}"
  [[ -n "$name" ]] && printf '%s\n' "$name"
}

migrate_workspace() {
  local home="$1"
  local ws="$2"
  local stores_dir="$home/.local/share/mmry/stores"
  local out="$ws/.mmry/mmry.jsonl"

  [[ -d "$stores_dir" ]] || return 0
  [[ -d "$ws" ]] || return 0

  if [[ -s "$out" ]]; then
    return 0
  fi

  local ws_base git_name
  ws_base="$(basename "$ws")"
  git_name="$(parse_remote_repo_name "$ws")"

  local candidates=()
  candidates+=("$ws_base")
  [[ -n "$git_name" && "$git_name" != "$ws_base" ]] && candidates+=("$git_name")
  candidates+=("default")

  local db chosen=""
  for c in "${candidates[@]}"; do
    db="$stores_dir/$c.db"
    if [[ -f "$db" ]]; then
      chosen="$db"
      break
    fi
  done

  [[ -n "$chosen" ]] || return 0

  mkdir -p "$(dirname "$out")"
  if python3 "$PY_MIGRATE" "$chosen" -o "$out" >/dev/null 2>&1; then
    if [[ -s "$out" ]]; then
      log "migrated: $ws <- $(basename "$chosen")"
    else
      rm -f "$out"
    fi
  else
    log "failed: $ws <- $(basename "$chosen")"
  fi
}

collect_homes() {
  local homes=()
  if [[ "$MODE" == "single" || "$MODE" == "auto" ]]; then
    homes+=("$HOME")
  fi
  if [[ "$MODE" == "multi" || "$MODE" == "auto" ]]; then
    while IFS= read -r h; do
      [[ -n "$h" ]] && homes+=("$h")
    done < <(find /home -maxdepth 1 -mindepth 1 -type d -name 'oqto_*' 2>/dev/null || true)
  fi
  printf '%s\n' "${homes[@]}" | awk 'NF' | sort -u
}

workspace_roots_for_home() {
  local home="$1"
  local user
  user="$(basename "$home")"

  # Defaults/fallbacks
  local roots=()
  roots+=("$home/oqto")

  # Try oqto config workspace_dir from both system and user locations.
  local cfg_candidates=(
    "/etc/oqto/config.toml"
    "$home/.config/oqto/config.toml"
    "/var/lib/oqto/.config/oqto/config.toml"
  )

  local cfg
  for cfg in "${cfg_candidates[@]}"; do
    [[ -f "$cfg" ]] || continue
    local parsed
    parsed=$(python3 - "$cfg" "$home" "$user" <<'PY'
import os, sys
from pathlib import Path

cfg = Path(sys.argv[1])
home = sys.argv[2]
user = sys.argv[3]

try:
    import tomllib
except Exception:
    print("")
    raise SystemExit(0)

try:
    data = tomllib.loads(cfg.read_text())
except Exception:
    print("")
    raise SystemExit(0)

v = data.get("local", {}).get("workspace_dir", "")
if not isinstance(v, str) or not v:
    print("")
    raise SystemExit(0)

v = v.replace("{user_id}", user).replace("{linux_username}", user).replace("{user}", user)
if v.startswith("$HOME"):
    v = v.replace("$HOME", home, 1)
if v.startswith("~/"):
    v = os.path.join(home, v[2:])
elif v == "~":
    v = home

print(v)
PY
)
    if [[ -n "$parsed" ]]; then
      roots+=("$parsed")
    fi
  done

  printf '%s\n' "${roots[@]}" | awk 'NF' | sort -u
}

migrated=0
while IFS= read -r home; do
  while IFS= read -r ws_root; do
    [[ -d "$ws_root" ]] || continue
    while IFS= read -r ws; do
      [[ -d "$ws" ]] || continue
      before=0
      [[ -s "$ws/.mmry/mmry.jsonl" ]] && before=1
      migrate_workspace "$home" "$ws"
      after=0
      [[ -s "$ws/.mmry/mmry.jsonl" ]] && after=1
      if [[ "$before" -eq 0 && "$after" -eq 1 ]]; then
        migrated=$((migrated + 1))
      fi
    done < <(find "$ws_root" -mindepth 1 -maxdepth 1 -type d 2>/dev/null || true)
  done < <(workspace_roots_for_home "$home")
done < <(collect_homes)

log "done (newly migrated workspaces: $migrated)"
