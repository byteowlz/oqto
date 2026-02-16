#!/usr/bin/env bash
set -euo pipefail

# Octo Reset Script
# Stops services, wipes databases and state, so setup.sh runs fresh.
#
# Usage:
#   ./scripts/reset.sh           # Interactive (confirms each step)
#   ./scripts/reset.sh --full    # Wipe everything including config
#   ./scripts/reset.sh --db      # Only wipe databases
#   ./scripts/reset.sh --state   # Only wipe setup state (re-run all steps)
#   ./scripts/reset.sh --users   # Only wipe users from database

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FULL_RESET="false"
DB_ONLY="false"
STATE_ONLY="false"
USERS_ONLY="false"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

log_info()  { echo -e "${BOLD}[INFO]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

confirm() {
  local prompt="${1:-Continue?}"
  echo -en "${BOLD}${prompt} [y/N]${NC} "
  read -r answer
  [[ "$answer" =~ ^[Yy]$ ]]
}

# Parse args
while [[ $# -gt 0 ]]; do
  case "$1" in
    --full)    FULL_RESET="true"; shift ;;
    --db)      DB_ONLY="true"; shift ;;
    --state)   STATE_ONLY="true"; shift ;;
    --users)   USERS_ONLY="true"; shift ;;
    --help|-h)
      echo "Usage: $0 [--full|--db|--state|--users]"
      echo ""
      echo "  --full    Wipe everything: services, databases, config, setup state"
      echo "  --db      Only wipe databases (keeps config and services)"
      echo "  --state   Only wipe setup step tracking (re-run all setup steps)"
      echo "  --users   Only delete all users from the database"
      echo "  (none)    Interactive mode - confirms each step"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"; exit 1 ;;
  esac
done

OCTO_CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/octo"
OCTO_DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/octo"
OCTO_STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/octo"
SERVICE_DATA_DIR="/var/lib/octo"

echo
echo -e "${BOLD}${RED}Octo Reset${NC}"
echo

# --- Users only ---
if [[ "$USERS_ONLY" == "true" ]]; then
  log_info "Deleting all users from database..."
  for db in "$SERVICE_DATA_DIR/.local/share/octo/sessions.db" "$OCTO_DATA_DIR/sessions.db" "$OCTO_DATA_DIR/octo.db"; do
    if [[ -f "$db" ]]; then
      if [[ "$db" == "$SERVICE_DATA_DIR"* ]]; then
        count=$(sudo sqlite3 "$db" "SELECT COUNT(*) FROM users;" 2>/dev/null || echo "0")
        log_info "  $db: $count users"
        sudo sqlite3 "$db" "DELETE FROM users;" 2>/dev/null && log_ok "  Cleared" || log_warn "  Failed"
      else
        count=$(sqlite3 "$db" "SELECT COUNT(*) FROM users;" 2>/dev/null || echo "0")
        log_info "  $db: $count users"
        sqlite3 "$db" "DELETE FROM users;" 2>/dev/null && log_ok "  Cleared" || log_warn "  Failed"
      fi
    fi
  done

  # Also remove Linux users created by octo
  log_info "Checking for octo_ Linux users..."
  for user in $(getent passwd | grep '^octo_' | cut -d: -f1); do
    log_info "  Removing Linux user: $user"
    sudo userdel -r "$user" 2>/dev/null || log_warn "  Failed to remove $user"
  done

  log_ok "Users reset complete. Re-run: ./setup.sh --redo admin_user_db"
  exit 0
fi

# --- State only ---
if [[ "$STATE_ONLY" == "true" ]]; then
  log_info "Wiping setup state..."
  rm -f "$OCTO_CONFIG_DIR/setup-state.env"
  rm -f "$OCTO_CONFIG_DIR/setup-steps-done"
  rm -f "$OCTO_CONFIG_DIR/.admin_setup"
  log_ok "Setup state cleared. Run ./setup.sh to redo all steps."
  exit 0
fi

# --- DB only ---
if [[ "$DB_ONLY" == "true" ]]; then
  log_info "Stopping octo service..."
  sudo systemctl stop octo 2>/dev/null || systemctl --user stop octo 2>/dev/null || true
  sleep 1

  log_info "Wiping databases..."
  for db in "$SERVICE_DATA_DIR/.local/share/octo/sessions.db" "$OCTO_DATA_DIR/sessions.db" "$OCTO_DATA_DIR/octo.db"; do
    if [[ -f "$db" ]]; then
      if [[ "$db" == "$SERVICE_DATA_DIR"* ]]; then
        sudo rm -f "$db" "${db}-wal" "${db}-shm"
      else
        rm -f "$db" "${db}-wal" "${db}-shm"
      fi
      log_ok "  Removed: $db"
    fi
  done

  log_info "Restarting octo service (will recreate DB on startup)..."
  sudo systemctl start octo 2>/dev/null || systemctl --user start octo 2>/dev/null || true
  log_ok "Databases reset. Re-run: ./setup.sh --redo admin_user_db"
  exit 0
fi

# --- Full or interactive ---
echo "This will:"
echo "  1. Stop all octo services"
echo "  2. Wipe databases"
if [[ "$FULL_RESET" == "true" ]]; then
  echo "  3. Wipe config and setup state"
  echo "  4. Remove octo_ Linux users"
fi
echo ""

if [[ "$FULL_RESET" != "true" ]]; then
  if ! confirm "Proceed with reset?"; then
    echo "Aborted."
    exit 0
  fi
fi

# 1. Stop services
log_info "Stopping services..."
sudo systemctl stop octo 2>/dev/null || true
sudo systemctl stop caddy 2>/dev/null || true
systemctl --user stop octo 2>/dev/null || true

# Stop any user runners
for user in $(getent passwd | grep '^octo_' | cut -d: -f1); do
  sudo -u "$user" XDG_RUNTIME_DIR="/run/user/$(id -u "$user" 2>/dev/null || echo 0)" \
    systemctl --user stop octo-runner 2>/dev/null || true
done
log_ok "Services stopped"

# 2. Wipe databases
log_info "Wiping databases..."
for db in "$SERVICE_DATA_DIR/.local/share/octo/sessions.db" "$OCTO_DATA_DIR/sessions.db" "$OCTO_DATA_DIR/octo.db"; do
  if [[ -f "$db" ]]; then
    if [[ "$db" == "$SERVICE_DATA_DIR"* ]]; then
      sudo rm -f "$db" "${db}-wal" "${db}-shm"
    else
      rm -f "$db" "${db}-wal" "${db}-shm"
    fi
    log_ok "  Removed: $db"
  fi
done

# 3. Wipe state and audit logs
log_info "Wiping state and logs..."
rm -rf "$OCTO_STATE_DIR" 2>/dev/null || true
sudo rm -rf "$SERVICE_DATA_DIR/.local/state/octo" 2>/dev/null || true
log_ok "State wiped"

# 4. Wipe setup tracking
log_info "Wiping setup state..."
rm -f "$OCTO_CONFIG_DIR/setup-steps-done"
rm -f "$OCTO_CONFIG_DIR/.admin_setup"
log_ok "Setup steps cleared"

if [[ "$FULL_RESET" == "true" ]]; then
  # 5. Wipe config
  log_info "Wiping config..."
  rm -f "$OCTO_CONFIG_DIR/config.toml"
  rm -f "$OCTO_CONFIG_DIR/env"
  rm -f "$OCTO_CONFIG_DIR/setup-state.env"
  sudo rm -rf "$SERVICE_DATA_DIR/.config/octo" 2>/dev/null || true
  sudo rm -f /etc/octo/config.toml 2>/dev/null || true
  log_ok "Config wiped"

  # 6. Remove octo_ Linux users
  log_info "Removing octo_ Linux users..."
  for user in $(getent passwd | grep '^octo_' | cut -d: -f1); do
    sudo userdel -r "$user" 2>/dev/null && log_ok "  Removed: $user" || log_warn "  Failed: $user"
  done
fi

echo
log_ok "Reset complete!"
echo
if [[ "$FULL_RESET" == "true" ]]; then
  echo "Run ./setup.sh to start fresh."
else
  echo "Run ./setup.sh --redo admin_user_db  to recreate admin user"
  echo "Run ./setup.sh --update              to rebuild and redeploy"
  echo "Run ./scripts/reset.sh --full        to also wipe config"
fi
