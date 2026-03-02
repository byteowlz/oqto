#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=proxmox-lib.sh
source "${SCRIPT_DIR}/proxmox-lib.sh"

TARGET=""
CREATE_SNAPSHOT="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="$2"
      shift 2
      ;;
    --create-snapshot)
      CREATE_SNAPSHOT="true"
      shift
      ;;
    *)
      echo "Usage: $0 --target ephemeral|continuous [--create-snapshot]" >&2
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

if [[ "$CREATE_SNAPSHOT" == "true" ]]; then
  if is_lxc; then
    proxmox_cmd "/usr/sbin/pct snapshot $VMID $OQTO_E2E_SNAPSHOT --description 'E2E baseline'" || true
  else
    proxmox_cmd "/usr/sbin/qm snapshot $VMID $OQTO_E2E_SNAPSHOT --description 'E2E baseline'" || true
  fi
  echo "Snapshot $OQTO_E2E_SNAPSHOT created for $VMID"
  exit 0
fi

if is_lxc; then
  proxmox_cmd "/usr/sbin/pct rollback $VMID $OQTO_E2E_SNAPSHOT" || {
    echo "Snapshot $OQTO_E2E_SNAPSHOT not found for $VMID" >&2
    exit 1
  }
  proxmox_cmd "/usr/sbin/pct start $VMID" >/dev/null 2>&1 || true
else
  proxmox_cmd "/usr/sbin/qm rollback $VMID $OQTO_E2E_SNAPSHOT" || {
    echo "Snapshot $OQTO_E2E_SNAPSHOT not found for $VMID" >&2
    exit 1
  }
  proxmox_cmd "/usr/sbin/qm start $VMID" >/dev/null 2>&1 || true
fi

echo "$VMID reset to snapshot $OQTO_E2E_SNAPSHOT"
