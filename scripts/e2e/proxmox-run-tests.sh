#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=proxmox-lib.sh
source "${SCRIPT_DIR}/proxmox-lib.sh"

TARGET=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="$2"
      shift 2
      ;;
    *)
      echo "Usage: $0 --target ephemeral|continuous" >&2
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

require_key

vm_start "$VMID"
IP=$(vm_wait_for_ip "$VMID")

health_url="http://${IP}:${OQTO_E2E_HTTP_PORT}${OQTO_E2E_HOST_CHECK_PATH}"
if ! curl -fsS "$health_url" >/dev/null; then
  echo "Health check failed: $health_url" >&2
  exit 1
fi

log_file=$(fetch_setup_log "$VMID")
admin_password=$(grep -E "Generated admin password:" "$log_file" | tail -n1 | sed "s/.*Generated admin password: //" || true)
if [[ -z "$admin_password" ]]; then
  admin_password=$(openssl rand -base64 18)
fi

login_payload=$(printf '{"username":"%s","password":"%s"}' "$OQTO_E2E_ADMIN_USERNAME" "$admin_password")
login_url="http://${IP}:${OQTO_E2E_HTTP_PORT}/api/auth/login"

login_response=$(curl -sS -H "Content-Type: application/json" -d "$login_payload" "$login_url" || true)
token=$(printf '%s' "$login_response" | python3 -c 'import json,sys; data=sys.stdin.read().strip();
import json
if not data:
    sys.exit(0)
try:
    payload=json.loads(data)
except json.JSONDecodeError:
    sys.exit(0)
print(payload.get("token", ""))')

if [[ -z "$token" ]]; then
  echo "Login response was empty or missing token" >&2
  if [[ -n "$login_response" ]]; then
    echo "Login response: $login_response" >&2
  fi
  db_path="/home/oqto/.local/share/oqto/oqto.db"
  vm_ssh "$IP" "sudo systemctl start oqto" >/dev/null
  vm_ssh "$IP" "for i in {1..30}; do sudo test -f ${db_path} && break; sleep 1; done"
  vm_ssh "$IP" "sudo /usr/local/bin/oqtoctl user set-password '${OQTO_E2E_ADMIN_USERNAME}' --password '${admin_password}' --config /home/oqto/.config/oqto/config.toml" >/dev/null || true

  login_response=$(curl -sS -H "Content-Type: application/json" -d "$login_payload" "$login_url" || true)
  token=$(printf '%s' "$login_response" | python3 -c 'import json,sys; data=sys.stdin.read().strip();
if not data:
    sys.exit(0)
try:
    payload=json.loads(data)
except json.JSONDecodeError:
    sys.exit(0)
print(payload.get("token", ""))')

  if [[ -z "$token" ]]; then
    echo "Login after password reset failed" >&2
    if [[ -n "$login_response" ]]; then
      echo "Login response: $login_response" >&2
    fi
    password_hash=$(vm_ssh "$IP" "sudo /usr/local/bin/oqtoctl hash-password --password '${admin_password}'")
    vm_ssh "$IP" "printf 'y\n' | sudo /usr/local/bin/oqtoctl user bootstrap --username '${OQTO_E2E_ADMIN_USERNAME}' --email 'e2e-admin@example.com' --database '${db_path}' --password-hash '${password_hash}' --no-linux-user --config /home/oqto/.config/oqto/config.toml" >/dev/null || true

    login_response=$(curl -sS -H "Content-Type: application/json" -d "$login_payload" "$login_url" || true)
    token=$(printf '%s' "$login_response" | python3 -c 'import json,sys; data=sys.stdin.read().strip();
if not data:
    sys.exit(0)
try:
    payload=json.loads(data)
except json.JSONDecodeError:
    sys.exit(0)
print(payload.get("token", ""))')
  fi
fi

if [[ -z "$token" ]]; then
  echo "Failed to obtain auth token" >&2
  exit 1
fi

auth_header="Authorization: Bearer ${token}"

workspace_suffix=$(openssl rand -hex 3)
create_payload=$(printf '{"name":"E2E SW %s","description":"E2E workspace"}' "$workspace_suffix")
create_url="http://${IP}:${OQTO_E2E_HTTP_PORT}/api/shared-workspaces"
workspace_json=$(curl -fsS -H "Content-Type: application/json" -H "$auth_header" -d "$create_payload" "$create_url")
workspace_id=$(printf '%s' "$workspace_json" | python3 -c 'import json,sys; data=json.loads(sys.stdin.read()); print(data.get("id", ""))')

if [[ -z "$workspace_id" ]]; then
  echo "Failed to create shared workspace" >&2
  exit 1
fi

curl -fsS -H "$auth_header" -X DELETE "http://${IP}:${OQTO_E2E_HTTP_PORT}/api/shared-workspaces/${workspace_id}" >/dev/null

echo "E2E smoke tests passed for VM $VMID ($IP)"
