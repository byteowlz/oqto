#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/migrate-linux-user.sh --target-user <linux_user> [options]

Options:
  --source-user <user>     Source user to migrate from (default: current user)
  --target-user <user>     Target Linux user to migrate to (required)
  --workspace-dir <path>   Relative workspace directory to migrate (optional)
  --config <path>          Config file to read workspace_dir from (optional)
  --dry-run                Show what would be copied without making changes
  -h, --help               Show this help

This script copies common Oqto user data directories from the source user's
home to the target user's home and fixes ownership. It does not delete any
existing files on the target.
USAGE
}

source_user="$(id -un)"
target_user=""
workspace_dir=""
config_path=""
dry_run="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-user)
      source_user="$2"
      shift 2
      ;;
    --target-user)
      target_user="$2"
      shift 2
      ;;
    --workspace-dir)
      workspace_dir="$2"
      shift 2
      ;;
    --config)
      config_path="$2"
      shift 2
      ;;
    --dry-run)
      dry_run="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
 done

if [[ -z "$target_user" ]]; then
  echo "--target-user is required" >&2
  usage >&2
  exit 1
fi

source_home="$(getent passwd "$source_user" | cut -d: -f6)"
target_home="$(getent passwd "$target_user" | cut -d: -f6)"
target_group="$(id -gn "$target_user")"

if [[ -z "$source_home" ]]; then
  echo "Source user not found: $source_user" >&2
  exit 1
fi

if [[ -z "$target_home" ]]; then
  echo "Target user not found: $target_user" >&2
  exit 1
fi

if [[ -z "$config_path" ]]; then
  config_path="$source_home/.config/oqto/config.toml"
fi

resolve_workspace_dir() {
  local raw="$1"

  raw="${raw//\$\{HOME\}/$source_home}"
  raw="${raw//\$HOME/$source_home}"
  raw="${raw/#\~/$source_home}"
  raw="${raw//\{linux_username\}/$source_user}"
  raw="${raw//\{user_id\}/$source_user}"

  if [[ -z "$raw" ]]; then
    return 1
  fi

  if [[ "$raw" = /* ]]; then
    if [[ "$raw" == "$source_home"* ]]; then
      echo "${raw#"$source_home"/}"
      return 0
    fi

    echo ""
    return 2
  fi

  echo "$raw"
  return 0
}

if [[ -z "$workspace_dir" && -f "$config_path" ]]; then
  raw_workspace_dir=$(awk '
    BEGIN { in_local = 0 }
    /^[[:space:]]*\[/ { in_local = 0 }
    /^[[:space:]]*\[local\][[:space:]]*$/ { in_local = 1; next }
    in_local && /^[[:space:]]*workspace_dir[[:space:]]*=/ {
      line = $0
      sub(/#.*/, "", line)
      sub(/^[[:space:]]*workspace_dir[[:space:]]*=[[:space:]]*/, "", line)
      gsub(/^[[:space:]]*"/, "", line)
      gsub(/"[[:space:]]*$/, "", line)
      gsub(/^[[:space:]]*'"'"'/, "", line)
      gsub(/'"'"'[[:space:]]*$/, "", line)
      print line
      exit
    }
  ' "$config_path")

  if [[ -n "$raw_workspace_dir" ]]; then
    if workspace_dir_resolved=$(resolve_workspace_dir "$raw_workspace_dir"); then
      workspace_dir="$workspace_dir_resolved"
    else
      if [[ $? -eq 2 ]]; then
        echo "Skipping workspace_dir from config (outside source home): $raw_workspace_dir" >&2
      fi
    fi
  fi
fi

paths=(
  ".local/share/oqto"
  ".local/share/pi"
  ".local/share/mmry"
  ".local/share/hstry"
  ".config/pi"
  ".config/mmry"
)

if [[ -n "$workspace_dir" ]]; then
  if [[ "$workspace_dir" = /* ]]; then
    echo "--workspace-dir must be a path relative to the home directory" >&2
    exit 1
  fi
  paths+=("$workspace_dir")
fi

run_cmd() {
  if [[ "$dry_run" == "true" ]]; then
    echo "DRY RUN: $*"
  else
    "$@"
  fi
}

copy_path() {
  local rel_path="$1"
  local src="$source_home/$rel_path"
  local dest="$target_home/$rel_path"

  if [[ ! -e "$src" ]]; then
    echo "Skipping missing path: $src"
    return 0
  fi

  echo "Copying $src -> $dest"
  run_cmd sudo install -d -m 0755 -o "$target_user" -g "$target_group" "$(dirname "$dest")"
  run_cmd sudo rsync -a "$src" "$(dirname "$dest")/"
  run_cmd sudo chown -R "$target_user":"$target_group" "$dest"
}

for rel_path in "${paths[@]}"; do
  copy_path "$rel_path"
done

echo "Migration complete. Review target home: $target_home"