#!/usr/bin/env bash
set -euo pipefail

# Native Lima workflow (no Docker-in-Lima)
#
# Commands:
#   ./deploy/lima/bootstrap.sh up [vm-name]
#   ./deploy/lima/bootstrap.sh setup [vm-name]
#   ./deploy/lima/bootstrap.sh ssh [vm-name]
#   ./deploy/lima/bootstrap.sh logs [vm-name]
#   ./deploy/lima/bootstrap.sh status [vm-name]
#   ./deploy/lima/bootstrap.sh down [vm-name]

CMD="${1:-up}"
VM_NAME="${2:-oqto}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TEMPLATE="${SCRIPT_DIR}/oqto.yaml"
SETUP_CONFIG="${SCRIPT_DIR}/oqto.setup.toml"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: missing dependency '$1'" >&2
    exit 1
  }
}

run_in_vm() {
  limactl shell "${VM_NAME}" -- "$@"
}

ensure_vm() {
  if ! limactl list --format json | jq -e ".[] | select(.name==\"${VM_NAME}\")" >/dev/null; then
    echo "[lima] starting VM '${VM_NAME}' from ${TEMPLATE}"
    limactl start --name "${VM_NAME}" "${TEMPLATE}"
  else
    local status
    status="$(limactl list --format json | jq -r ".[] | select(.name==\"${VM_NAME}\") | .status")"
    if [[ "${status}" != "Running" ]]; then
      echo "[lima] starting existing VM '${VM_NAME}'"
      limactl start "${VM_NAME}"
    fi
  fi
}

run_setup() {
  run_in_vm bash -lc "
    set -euo pipefail
    cd '${REPO_ROOT}'
    ./setup.sh --config '${SETUP_CONFIG}'
  "

  echo
  echo "[oqto] setup complete"
  echo "[oqto] URL: http://localhost:8086"
}

case "${CMD}" in
  up)
    require limactl
    require jq
    ensure_vm
    ;;
  setup)
    require limactl
    require jq
    ensure_vm
    run_setup
    ;;
  ssh)
    require limactl
    exec limactl shell "${VM_NAME}"
    ;;
  logs)
    require limactl
    run_in_vm bash -lc "sudo journalctl -u oqto -f"
    ;;
  status)
    require limactl
    limactl list
    echo
    run_in_vm bash -lc "systemctl status oqto --no-pager || true"
    ;;
  down)
    require limactl
    limactl stop "${VM_NAME}" || true
    ;;
  *)
    echo "usage: $0 {up|setup|ssh|logs|status|down} [vm-name]" >&2
    exit 1
    ;;
esac
