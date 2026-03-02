#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=proxmox-lib.sh
source "${SCRIPT_DIR}/proxmox-lib.sh"

require_key

prepare_vm() {
  local vmid="$1"
  if is_lxc; then
    echo "LXC mode: skip guest agent setup for ${vmid}"
    return 0
  fi
  vm_set_agent "$vmid"
  vm_start "$vmid"
  local ip
  ip=$(vm_wait_for_ip "$vmid")
  vm_wait_for_ssh "$ip"
  if [[ "$OQTO_E2E_ENABLE_NOPASSWD" == "true" ]]; then
    vm_enable_passwordless_sudo "$ip"
  fi
  vm_install_guest_agent "$ip"
}

prepare_vm "$VM_EPHEMERAL"
prepare_vm "$VM_CONTINUOUS"

echo "Prepare complete for ${VM_EPHEMERAL} and ${VM_CONTINUOUS}"
