# ==============================================================================
# Setup State Persistence
# ==============================================================================
#
# Saves all interactive decisions to a state file so re-runs don't require
# re-entering everything. State is stored in ~/.config/oqto/setup-state.env
# and loaded automatically on subsequent runs.
#
# Usage:
#   ./setup.sh              # Loads previous state, prompts to reuse
#   ./setup.sh --fresh      # Ignore saved state, start from scratch
#

SETUP_STATE_FILE="${XDG_CONFIG_HOME}/oqto/setup-state.env"

# Keys that are persisted (order matters for display)
SETUP_STATE_KEYS=(
  SELECTED_USER_MODE
  SELECTED_BACKEND_MODE
  PRODUCTION_MODE
  OQTO_DEV_MODE
  WORKSPACE_DIR
  DOMAIN
  SETUP_CADDY
  ADMIN_USERNAME
  ADMIN_EMAIL
  dev_user_id
  dev_user_name
  dev_user_email
  INSTALL_ALL_TOOLS
  INSTALL_MMRY
  OQTO_HARDEN_SERVER
  OQTO_SSH_PORT
  OQTO_SETUP_FIREWALL
  OQTO_SETUP_FAIL2BAN
  OQTO_HARDEN_SSH
  OQTO_SETUP_AUTO_UPDATES
  OQTO_HARDEN_KERNEL
  CONTAINER_RUNTIME
  JWT_SECRET
  EAVS_MASTER_KEY
  CONFIGURED_PROVIDERS
)

# Save current decisions to state file
save_setup_state() {
  mkdir -p "$(dirname "$SETUP_STATE_FILE")"

  {
    echo "# Oqto setup state - generated $(date)"
    echo "# This file is loaded on re-runs to avoid re-entering decisions."
    echo "# Delete this file or run ./setup.sh --fresh to start over."
    echo ""
    for key in "${SETUP_STATE_KEYS[@]}"; do
      local val="${!key:-}"
      if [[ -n "$val" ]]; then
        echo "${key}=$(printf '%q' "$val")"
      fi
    done
  } >"$SETUP_STATE_FILE"
  chmod 600 "$SETUP_STATE_FILE"
  log_success "Setup state saved to $SETUP_STATE_FILE"
}

# Load previous state and offer to reuse it
load_setup_state() {
  if [[ ! -f "$SETUP_STATE_FILE" ]]; then
    return 1
  fi

  echo -e "${BOLD}Previous setup state found:${NC}"
  echo ""

  # Show key decisions (skip secrets)
  local secrets_regex="^(JWT_SECRET|EAVS_MASTER_KEY)$"
  while IFS='=' read -r key val; do
    # Skip comments and empty lines
    [[ "$key" =~ ^#.*$ || -z "$key" ]] && continue
    # Unescape the value
    val=$(eval "echo $val" 2>/dev/null || echo "$val")
    if [[ "$key" =~ $secrets_regex ]]; then
      echo -e "  ${CYAN}${key}${NC} = ****"
    else
      echo -e "  ${CYAN}${key}${NC} = ${val}"
    fi
  done <"$SETUP_STATE_FILE"

  # Show completed steps
  if [[ -f "$SETUP_STEPS_FILE" ]]; then
    local step_count
    step_count=$(wc -l <"$SETUP_STEPS_FILE")
    echo -e "  ${GREEN}Completed steps:${NC} $step_count"
    echo ""
  fi

  return 0
}

# Source the state file to restore variables
apply_setup_state() {
  if [[ -f "$SETUP_STATE_FILE" ]]; then
    # Fix known typos from previous versions before sourcing
    # Fix known typos and outdated defaults from previous versions
    sed -i 's|/home/{user_id/oqto}|/home/{linux_username}/oqto|g' "$SETUP_STATE_FILE" 2>/dev/null || true
    sed -i 's|/home/{user_id}/oqto|/home/{linux_username}/oqto|g' "$SETUP_STATE_FILE" 2>/dev/null || true
    # shellcheck source=/dev/null
    source "$SETUP_STATE_FILE"
    log_success "Loaded previous setup state"
  fi
}

