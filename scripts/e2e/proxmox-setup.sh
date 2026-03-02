#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=proxmox-lib.sh
source "${SCRIPT_DIR}/proxmox-lib.sh"

TARGET=""
MODE="toml"
CONFIG_FILE="$OQTO_E2E_SETUP_CONFIG"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="$2"
      shift 2
      ;;
    --mode)
      MODE="$2"
      shift 2
      ;;
    --config)
      CONFIG_FILE="$2"
      shift 2
      ;;
    *)
      echo "Usage: $0 --target ephemeral|continuous [--mode toml|url] [--config path]" >&2
      exit 1
      ;;
  esac
done

if [[ "$TARGET" == "ephemeral" ]]; then
  VMID="$VM_EPHEMERAL"
elif [[ "$TARGET" == "continuous" ]]; then
  VMID="$VM_CONTINUOUS"
else
  echo "Invalid target: $TARGET" >&2
  exit 1
fi

if [[ ! -f "$CONFIG_FILE" ]]; then
  echo "Missing setup config: $CONFIG_FILE" >&2
  exit 1
fi

require_key

vm_start "$VMID"
IP=$(vm_wait_for_ip "$VMID")
vm_wait_for_ssh "$IP"

vm_sync_repo "$IP"
run_setup "$IP" "$VMID" "$MODE" "$CONFIG_FILE"

echo "Setup completed on VM $VMID ($IP)"
