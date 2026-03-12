#!/usr/bin/env bash
# sync-agent-instructions.sh - Sync ~/.pi/agent/AGENTS.md to platform users
#
# This script is intentionally filesystem-based (not DB-based) so it can be
# used for surgical rollout across all local platform users, including hosts
# where users exist but are not represented in oqto DB state.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TARGET_ALL=false
TARGET_USER=""
SOURCE_FILE=""
UPDATE_SKEL=false
DRY_RUN=false

usage() {
  cat <<EOF
sync-agent-instructions - Sync AGENTS.md into user ~/.pi/agent directories

USAGE:
  sync-agent-instructions.sh --all [--source <file>] [--update-skel] [--dry-run]
  sync-agent-instructions.sh --user <linux_username> [--source <file>] [--dry-run]

OPTIONS:
  --all, -a             Sync for all detected platform users (/home/oqto_* and /home/octo_*)
  --user, -u <name>     Sync for one linux user (home expected at /home/<name>)
  --source <file>       Source AGENTS.md file
  --update-skel         Also copy to /etc/skel/.pi/agent/AGENTS.md
  --dry-run             Print actions only
  -h, --help            Show this help

Default source resolution:
  1) ../oqto-templates/dotfiles/.pi/agent/AGENTS.md
  2) backend/crates/oqto/src/templates/embedded/AGENTS.md
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --all|-a)
      TARGET_ALL=true
      shift
      ;;
    --user|-u)
      TARGET_USER="$2"
      shift 2
      ;;
    --source)
      SOURCE_FILE="$2"
      shift 2
      ;;
    --update-skel)
      UPDATE_SKEL=true
      shift
      ;;
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ "$TARGET_ALL" == true && -n "$TARGET_USER" ]]; then
  echo "Use either --all or --user, not both" >&2
  exit 1
fi
if [[ "$TARGET_ALL" == false && -z "$TARGET_USER" ]]; then
  echo "Specify --all or --user" >&2
  exit 1
fi

resolve_source() {
  if [[ -n "$SOURCE_FILE" ]]; then
    if [[ ! -f "$SOURCE_FILE" ]]; then
      echo "Source file not found: $SOURCE_FILE" >&2
      exit 1
    fi
    echo "$SOURCE_FILE"
    return
  fi

  local candidate1="$REPO_ROOT/../oqto-templates/dotfiles/.pi/agent/AGENTS.md"
  local candidate2="$REPO_ROOT/backend/crates/oqto/src/templates/embedded/AGENTS.md"

  if [[ -f "$candidate1" ]]; then
    echo "$candidate1"
    return
  fi
  if [[ -f "$candidate2" ]]; then
    echo "$candidate2"
    return
  fi

  echo "Could not resolve default source AGENTS.md" >&2
  exit 1
}

sync_one_home() {
  local home="$1"
  local source="$2"

  if [[ ! -d "$home" ]]; then
    echo "[WARN] Home does not exist: $home" >&2
    return 1
  fi

  local user
  user="$(basename "$home")"

  local group
  group="$(stat -c %G "$home" 2>/dev/null || echo "")"
  if [[ -z "$group" ]]; then
    group="$user"
  fi

  local target_dir="$home/.pi/agent"
  local target_file="$target_dir/AGENTS.md"

  if [[ "$DRY_RUN" == true ]]; then
    echo "[dry-run] install -d $target_dir"
    echo "[dry-run] cp $source -> $target_file"
    echo "[dry-run] chown $user:$group $target_file"
    return 0
  fi

  sudo install -d -m 755 "$target_dir"
  sudo cp "$source" "$target_file"
  sudo chown "$user:$group" "$target_file"
  echo "[OK] $user -> $target_file"
}

SOURCE="$(resolve_source)"
echo "Using source: $SOURCE"

if [[ "$TARGET_ALL" == true ]]; then
  updated=0
  for home in /home/oqto_* /home/octo_*; do
    [[ -d "$home" ]] || continue
    sync_one_home "$home" "$SOURCE" && updated=$((updated + 1))
  done
  echo "Updated users: $updated"

  if [[ "$UPDATE_SKEL" == true ]]; then
    if [[ "$DRY_RUN" == true ]]; then
      echo "[dry-run] install -d /etc/skel/.pi/agent"
      echo "[dry-run] cp $SOURCE -> /etc/skel/.pi/agent/AGENTS.md"
    else
      sudo install -d -m 755 /etc/skel/.pi/agent
      sudo cp "$SOURCE" /etc/skel/.pi/agent/AGENTS.md
      echo "[OK] updated /etc/skel/.pi/agent/AGENTS.md"
    fi
  fi
else
  sync_one_home "/home/$TARGET_USER" "$SOURCE"
fi
