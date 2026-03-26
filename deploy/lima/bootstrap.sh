#!/usr/bin/env bash
set -euo pipefail

# Easy Lima workflow for Oqto Docker dev runtime.
#
# Commands:
#   ./deploy/lima/bootstrap.sh up [vm-name]
#   ./deploy/lima/bootstrap.sh ssh [vm-name]
#   ./deploy/lima/bootstrap.sh logs [vm-name]
#   ./deploy/lima/bootstrap.sh status [vm-name]
#   ./deploy/lima/bootstrap.sh down [vm-name]

CMD="${1:-up}"
VM_NAME="${2:-oqto}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TEMPLATE="${SCRIPT_DIR}/oqto.yaml"

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

start_oqto() {
  local repo_in_vm="${REPO_ROOT}"

  run_in_vm bash -lc "
    set -euo pipefail

    if [ ! -d '${repo_in_vm}/deploy/docker' ]; then
      echo 'error: repo path not mounted in VM:' '${repo_in_vm}' >&2
      echo 'hint: ensure your repo is inside your home directory (mounted by Lima).' >&2
      exit 1
    fi

    cd '${repo_in_vm}/deploy/docker'

    if [ ! -f .env ]; then
      cp .env.example .env
    fi

    # Docker mode defaults to single-user. Keep that explicit.
    if ! grep -q '^OQTO_SINGLE_USER=' .env; then
      echo 'OQTO_SINGLE_USER=true' >> .env
    else
      sed -i 's/^OQTO_SINGLE_USER=.*/OQTO_SINGLE_USER=true/' .env
    fi

    # Make login deterministic for local dev.
    if ! grep -q '^ADMIN_USER=' .env; then
      echo 'ADMIN_USER=admin' >> .env
    fi
    if ! grep -q '^ADMIN_PASSWORD=' .env; then
      echo 'ADMIN_PASSWORD=admin123456' >> .env
    elif grep -q '^ADMIN_PASSWORD=$' .env; then
      sed -i 's/^ADMIN_PASSWORD=$/ADMIN_PASSWORD=admin123456/' .env
    fi

    docker compose build
    docker compose up -d
  "

  echo
  echo "[oqto] started in VM '${VM_NAME}'"
  echo "[oqto] URL: http://localhost:8086"
  echo "[oqto] Login: admin / admin123456 (unless overridden in deploy/docker/.env)"
}

case "${CMD}" in
  up)
    require limactl
    require jq
    ensure_vm
    start_oqto
    ;;
  ssh)
    require limactl
    exec limactl shell "${VM_NAME}"
    ;;
  logs)
    require limactl
    run_in_vm bash -lc "cd '${REPO_ROOT}/deploy/docker' && docker compose logs -f"
    ;;
  status)
    require limactl
    limactl list
    echo
    run_in_vm bash -lc "cd '${REPO_ROOT}/deploy/docker' && docker compose ps" || true
    ;;
  down)
    require limactl
    run_in_vm bash -lc "cd '${REPO_ROOT}/deploy/docker' && docker compose down" || true
    limactl stop "${VM_NAME}" || true
    ;;
  *)
    echo "usage: $0 {up|ssh|logs|status|down} [vm-name]" >&2
    exit 1
    ;;
esac
