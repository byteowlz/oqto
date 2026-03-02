#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=proxmox-lib.sh
source "${SCRIPT_DIR}/proxmox-lib.sh"

require_key

KEY_CONTENT=$(cat "$OQTO_E2E_SSH_KEY.pub")

create_ct() {
  local vmid="$1"
  local hostname="$2"

  proxmox_cmd "cat > /tmp/oqto-e2e-key.pub <<'EOF'
${KEY_CONTENT}
EOF"

  proxmox_cmd "/usr/sbin/qm stop ${vmid}" >/dev/null 2>&1 || true
  proxmox_cmd "/usr/sbin/qm destroy ${vmid} --purge" >/dev/null 2>&1 || true
  proxmox_cmd "/usr/sbin/pct stop ${vmid}" >/dev/null 2>&1 || true
  proxmox_cmd "/usr/sbin/pct destroy ${vmid} --purge" >/dev/null 2>&1 || true

  local rootfs_arg="${OQTO_E2E_STORAGE}:${OQTO_E2E_ROOTFS_SIZE}"

  proxmox_cmd "/usr/sbin/pct create ${vmid} ${OQTO_E2E_LXC_TEMPLATE} \
    --hostname ${hostname} \
    --storage ${OQTO_E2E_STORAGE} \
    --rootfs ${rootfs_arg} \
    --cores ${OQTO_E2E_CT_CORES} \
    --memory ${OQTO_E2E_CT_MEMORY_MB} \
    --swap ${OQTO_E2E_CT_SWAP_MB} \
    --net0 name=eth0,bridge=${OQTO_E2E_BRIDGE},ip=dhcp \
    --features nesting=1,keyctl=1 \
    --unprivileged 1 \
    --ssh-public-keys /tmp/oqto-e2e-key.pub"

  proxmox_cmd "/usr/sbin/pct start ${vmid}"

  local ip
  ip=$(lxc_wait_for_ip "$vmid")

  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- bash -lc 'apt-get update && apt-get install -y openssh-server sudo git curl docker.io'"
  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- bash -lc 'systemctl enable --now ssh'"
  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- bash -lc 'systemctl enable --now docker'"

  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- bash -lc 'useradd -m -s /bin/bash ${OQTO_E2E_SSH_USER} || true'"
  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- bash -lc 'usermod -aG sudo ${OQTO_E2E_SSH_USER}'"
  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- bash -lc 'usermod -aG docker ${OQTO_E2E_SSH_USER}'"
  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- bash -lc 'mkdir -p /home/${OQTO_E2E_SSH_USER}/.ssh && chown -R ${OQTO_E2E_SSH_USER}:${OQTO_E2E_SSH_USER} /home/${OQTO_E2E_SSH_USER}/.ssh'"
  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- bash -lc 'cat /root/.ssh/authorized_keys > /home/${OQTO_E2E_SSH_USER}/.ssh/authorized_keys && chown ${OQTO_E2E_SSH_USER}:${OQTO_E2E_SSH_USER} /home/${OQTO_E2E_SSH_USER}/.ssh/authorized_keys'"
  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- bash -lc 'chmod 700 /home/${OQTO_E2E_SSH_USER}/.ssh && chmod 600 /home/${OQTO_E2E_SSH_USER}/.ssh/authorized_keys'"
  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- bash -lc 'echo \"${OQTO_E2E_SSH_USER} ALL=(ALL) NOPASSWD:ALL\" > /etc/sudoers.d/oqto-e2e && chmod 440 /etc/sudoers.d/oqto-e2e'"

  echo "Created LXC ${vmid} (${hostname}) at ${ip}"
}

create_ct "$VM_EPHEMERAL" "oqto-e2e-ephemeral"
create_ct "$VM_CONTINUOUS" "oqto-e2e-continuous"
