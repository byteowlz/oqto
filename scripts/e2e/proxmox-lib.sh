#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=proxmox-env.sh
source "${SCRIPT_DIR}/proxmox-env.sh"

require_key() {
  if [[ ! -f "$OQTO_E2E_SSH_KEY" ]]; then
    echo "Missing SSH key: $OQTO_E2E_SSH_KEY" >&2
    exit 1
  fi
}

proxmox_cmd() {
  ssh -o BatchMode=yes -o StrictHostKeyChecking=no "$PROXMOX_HOST" "$@"
}

is_lxc() {
  [[ "$OQTO_E2E_MODE" == "lxc" ]]
}

vm_start() {
  local vmid="$1"
  if is_lxc; then
    proxmox_cmd "/usr/sbin/pct start $vmid" >/dev/null 2>&1 || true
  else
    proxmox_cmd "/usr/sbin/qm start $vmid" >/dev/null 2>&1 || true
  fi
}

vm_stop() {
  local vmid="$1"
  if is_lxc; then
    proxmox_cmd "/usr/sbin/pct stop $vmid" >/dev/null 2>&1 || true
  else
    proxmox_cmd "/usr/sbin/qm stop $vmid" >/dev/null 2>&1 || true
  fi
}

vm_set_agent() {
  local vmid="$1"
  if is_lxc; then
    return 0
  fi
  proxmox_cmd "/usr/sbin/qm set $vmid --agent enabled=1" >/dev/null
}

vm_get_mac() {
  local vmid="$1"
  proxmox_cmd "/usr/sbin/qm config $vmid" | awk -F'[=,]' '/^net0:/ {print $2}'
}

vm_ip_from_agent() {
  local vmid="$1"
  if is_lxc; then
    return 1
  fi
  local json
  json=$(proxmox_cmd "pvesh get /nodes/${PROXMOX_NODE}/qemu/${vmid}/agent/network-get-interfaces --output-format json" 2>/dev/null || true)
  if [[ -z "$json" ]]; then
    return 1
  fi

  python3 - <<'PY'
import json
import sys

data = json.loads(sys.stdin.read())
for iface in data:
    for ip in iface.get("ip-addresses", []):
        if ip.get("ip-address-type") == "ipv4":
            addr = ip.get("ip-address", "")
            if addr and not addr.startswith("127."):
                print(addr)
                sys.exit(0)
PY
}

vm_ip_from_mac() {
  local mac="$1"
  if [[ -z "$mac" ]]; then
    return 1
  fi
  proxmox_cmd "ip -4 neigh | grep -i '$mac' | awk '{print \$1}' | head -n1"
}

lxc_ip() {
  local vmid="$1"
  proxmox_cmd "/usr/sbin/pct exec ${vmid} -- ip -4 -o addr show dev eth0 | awk '{print \$4}' | cut -d/ -f1"
}

lxc_wait_for_ip() {
  local vmid="$1"
  local ip
  local elapsed=0

  while [[ $elapsed -lt $OQTO_E2E_WAIT_SECS ]]; do
    ip=$(lxc_ip "$vmid" 2>/dev/null | head -n1)
    if [[ -n "$ip" ]]; then
      echo "$ip"
      return 0
    fi
    sleep 3
    elapsed=$((elapsed + 3))
  done

  echo "Failed to resolve IP for container $vmid" >&2
  return 1
}

vm_wait_for_ip() {
  local vmid="$1"
  local mac
  local ip
  local elapsed=0

  if [[ "$vmid" == "$VM_EPHEMERAL" && -n "$OQTO_E2E_IP_EPHEMERAL" ]]; then
    echo "$OQTO_E2E_IP_EPHEMERAL"
    return 0
  fi
  if [[ "$vmid" == "$VM_CONTINUOUS" && -n "$OQTO_E2E_IP_CONTINUOUS" ]]; then
    echo "$OQTO_E2E_IP_CONTINUOUS"
    return 0
  fi

  if is_lxc; then
    lxc_wait_for_ip "$vmid"
    return $?
  fi

  mac=$(vm_get_mac "$vmid")
  while [[ $elapsed -lt $OQTO_E2E_WAIT_SECS ]]; do
    ip=$(vm_ip_from_agent "$vmid" 2>/dev/null || true)
    if [[ -z "$ip" ]]; then
      ip=$(vm_ip_from_mac "$mac" 2>/dev/null || true)
    fi
    if [[ -n "$ip" ]]; then
      echo "$ip"
      return 0
    fi
    sleep 3
    elapsed=$((elapsed + 3))
  done

  echo "Failed to resolve IP for VM $vmid" >&2
  return 1
}

vm_ssh() {
  local ip="$1"
  shift
  ssh -i "$OQTO_E2E_SSH_KEY" \
    -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile=/dev/null \
    "${OQTO_E2E_SSH_USER}@${ip}" "$@"
}

vm_wait_for_ssh() {
  local ip="$1"
  local elapsed=0
  while [[ $elapsed -lt $OQTO_E2E_WAIT_SECS ]]; do
    if vm_ssh "$ip" "echo ok" >/dev/null 2>&1; then
      return 0
    fi
    sleep 3
    elapsed=$((elapsed + 3))
  done
  echo "Timeout waiting for SSH on $ip" >&2
  return 1
}

sudo_prefix() {
  if [[ -n "$OQTO_E2E_SUDO_PASSWORD" ]]; then
    printf "echo '%s' | sudo -S" "$OQTO_E2E_SUDO_PASSWORD"
  else
    echo "sudo -n"
  fi
}

vm_enable_passwordless_sudo() {
  local ip="$1"
  local sudo_cmd
  sudo_cmd=$(sudo_prefix)
  vm_ssh "$ip" "${sudo_cmd} mkdir -p /etc/sudoers.d && ${sudo_cmd} bash -lc 'echo \"${OQTO_E2E_SSH_USER} ALL=(ALL) NOPASSWD:ALL\" > /etc/sudoers.d/oqto-e2e && chmod 440 /etc/sudoers.d/oqto-e2e'"
}

vm_install_guest_agent() {
  local ip="$1"
  local sudo_cmd
  sudo_cmd=$(sudo_prefix)
  vm_ssh "$ip" "${sudo_cmd} apt-get update && ${sudo_cmd} apt-get install -y qemu-guest-agent && ${sudo_cmd} systemctl enable --now qemu-guest-agent"
}

vm_sync_repo() {
  local ip="$1"
  rsync -az --delete \
    --exclude ".git" \
    --exclude "target" \
    --exclude "frontend/node_modules" \
    --exclude "backend/target" \
    -e "ssh -i $OQTO_E2E_SSH_KEY -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null" \
    "${SCRIPT_DIR}/../.." "${OQTO_E2E_SSH_USER}@${ip}:${OQTO_E2E_REPO_DIR}"
}

generate_setup_url() {
  local config_file="$1"
  python3 - "$config_file" <<'PY'
import base64
import sys

with open(sys.argv[1], "rb") as f:
    content = f.read()

encoded = base64.urlsafe_b64encode(content).decode("utf-8").rstrip("=")
print(f"https://oqto.dev/setup#{encoded}")
PY
}

run_setup() {
  local ip="$1"
  local vmid="$2"
  local mode="$3"
  local config_file="$4"

  mkdir -p "$OQTO_E2E_LOG_DIR"
  local log_file="$OQTO_E2E_LOG_DIR/setup-${vmid}.log"

  local cmd="cd ${OQTO_E2E_REPO_DIR} && chmod +x setup.sh && OQTO_FORCE_SOURCE_BUILD=${OQTO_E2E_FORCE_SOURCE_BUILD} CARGO_BUILD_JOBS=${OQTO_E2E_CARGO_BUILD_JOBS} CARGO_PROFILE_RELEASE_LTO=${OQTO_E2E_RELEASE_LTO} CARGO_PROFILE_RELEASE_CODEGEN_UNITS=${OQTO_E2E_RELEASE_CODEGEN_UNITS} ./setup.sh --non-interactive --fresh"
  if [[ "$mode" == "url" ]]; then
    local url
    url=$(generate_setup_url "$config_file")
    cmd="$cmd --from-url '$url'"
  else
    cmd="$cmd --config '$config_file'"
  fi

  local sudo_cmd
  sudo_cmd=$(sudo_prefix)
  vm_ssh "$ip" "${sudo_cmd} -E bash -lc \"$cmd\"" | tee "$log_file"
}

fetch_setup_log() {
  local vmid="$1"
  local log_file="$OQTO_E2E_LOG_DIR/setup-${vmid}.log"
  if [[ -f "$log_file" ]]; then
    echo "$log_file"
    return 0
  fi
  echo "Missing setup log for $vmid" >&2
  return 1
}
