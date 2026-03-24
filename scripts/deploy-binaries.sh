#!/usr/bin/env bash
# Quick binary-only deploy wrapper for oqto hosts.
#
# Usage examples:
#   ./scripts/deploy-binaries.sh --host octo-azure
#   ./scripts/deploy-binaries.sh --host octo-azure --host zbook --build
#   ./scripts/deploy-binaries.sh --host octo-azure --no-restart
#   ./scripts/deploy-binaries.sh --all --dry-run

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEPLOY_SCRIPT="$ROOT_DIR/scripts/deploy.sh"

if [[ ! -x "$DEPLOY_SCRIPT" ]]; then
  echo "Error: deploy script not executable: $DEPLOY_SCRIPT" >&2
  exit 1
fi

declare -a HOSTS=()
DRY_RUN=false
BUILD=false
RESTART=true
CONFIG=""

usage() {
  cat <<'EOF'
Quickly deploy backend binaries to one or more hosts.

Options:
  --host NAME       Deploy only to this host (repeatable)
  --all             Deploy to all hosts from deploy/hosts.toml
  --build           Build binaries before deploy (default: off / skip-build)
  --no-restart      Deploy binaries but skip service restarts
  --dry-run         Show commands without executing
  --config FILE     Alternate deploy hosts config file
  -h, --help        Show this help

Notes:
  - This always skips frontend deployment.
  - Binary list + service mapping are taken from deploy/hosts.toml.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      HOSTS+=("$2")
      shift 2
      ;;
    --all)
      shift
      ;;
    --build)
      BUILD=true
      shift
      ;;
    --no-restart)
      RESTART=false
      shift
      ;;
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    --config)
      CONFIG="$2"
      shift 2
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

args=(--skip-frontend)

if ! $BUILD; then
  args+=(--skip-build)
fi

if ! $RESTART; then
  args+=(--skip-services)
fi

if $DRY_RUN; then
  args+=(--dry-run)
fi

if [[ -n "$CONFIG" ]]; then
  args+=(--config "$CONFIG")
fi

for h in "${HOSTS[@]}"; do
  args+=(--host "$h")
done

exec "$DEPLOY_SCRIPT" "${args[@]}"
