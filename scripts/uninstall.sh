#!/usr/bin/env bash
set -euo pipefail

# Oqto uninstall helper (Linux)
# - Stops/disables oqto-related services
# - Removes installed oqto binaries and service units
# - Optional: purge data/config directories

PURGE_DATA=false
FORCE=false

for arg in "$@"; do
  case "$arg" in
    --purge-data) PURGE_DATA=true ;;
    --force|-y) FORCE=true ;;
    --help|-h)
      cat <<'EOF'
Usage: ./scripts/uninstall.sh [--purge-data] [--force]

Options:
  --purge-data   Also delete persistent data/config directories.
  --force, -y    Skip confirmation prompt.

Notes:
  - This script targets oqto-managed artifacts.
  - It does NOT uninstall Rust/Bun/system packages globally.
EOF
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

log() { echo "[oqto-uninstall] $*"; }

require_linux() {
  if [[ "$(uname -s)" != "Linux" ]]; then
    echo "This uninstall script currently supports Linux only." >&2
    exit 1
  fi
}

run_maybe_sudo() {
  if [[ "${EUID}" -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

stop_disable_system_service() {
  local svc="$1"
  if systemctl list-unit-files "$svc" >/dev/null 2>&1; then
    run_maybe_sudo systemctl disable --now "$svc" >/dev/null 2>&1 || true
    run_maybe_sudo rm -f "/etc/systemd/system/$svc" || true
    run_maybe_sudo rm -f "/usr/lib/systemd/system/$svc" || true
  fi
}

stop_disable_user_service_current() {
  local unit="$1"
  if systemctl --user list-unit-files "$unit" >/dev/null 2>&1; then
    systemctl --user disable --now "$unit" >/dev/null 2>&1 || true
  fi
  rm -f "$HOME/.config/systemd/user/$unit" || true
}

require_linux

if [[ "$FORCE" != true ]]; then
  echo "This will uninstall oqto services/binaries from this machine."
  echo "Use --purge-data to also delete /var/lib/oqto and config dirs."
  read -r -p "Continue? [y/N] " reply
  [[ "$reply" =~ ^[Yy]$ ]] || { log "Cancelled"; exit 0; }
fi

log "Stopping/removing system services"
for svc in \
  oqto.service \
  oqto-usermgr.service \
  oqto-healthcheck.service \
  oqto-healthcheck.timer \
  oqto-runner.service \
  oqto-runner.socket \
  eavs.service \
  caddy.service \
  hstry.service \
  mmry-embeddings.service; do
  stop_disable_system_service "$svc"
done

run_maybe_sudo systemctl daemon-reload || true

log "Stopping/removing user services for current user"
for unit in \
  oqto-user.service \
  oqto-runner.service \
  oqto-runner.socket \
  eavs.service \
  hstry.service; do
  stop_disable_user_service_current "$unit"
done
systemctl --user daemon-reload >/dev/null 2>&1 || true

log "Removing oqto binaries"
for bin in \
  /usr/local/bin/oqto \
  /usr/local/bin/oqtoctl \
  /usr/local/bin/oqto-runner \
  /usr/local/bin/oqto-browser \
  /usr/local/bin/oqto-browserd \
  /usr/local/bin/oqto-sandbox \
  /usr/local/bin/pi-bridge \
  /usr/local/bin/oqto-files \
  /usr/local/bin/oqto-usermgr \
  /usr/local/bin/oqto-scaffold \
  /usr/local/bin/oqto-setup; do
  run_maybe_sudo rm -f "$bin" || true
done

if [[ "$PURGE_DATA" == true ]]; then
  log "Purging persistent data/config"
  run_maybe_sudo rm -rf /var/lib/oqto || true
  run_maybe_sudo rm -rf /etc/oqto || true
  run_maybe_sudo rm -rf /etc/oqto-runner || true
  run_maybe_sudo rm -rf /etc/eavs || true
  rm -rf "$HOME/.config/oqto" "$HOME/.local/share/oqto" || true
  rm -rf "$HOME/.config/eavs" "$HOME/.local/share/eavs" || true
else
  log "Keeping data/config directories (use --purge-data to remove)"
fi

log "Done"
