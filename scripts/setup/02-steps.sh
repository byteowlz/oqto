# ==============================================================================
# Step Tracking
# ==============================================================================
#
# Tracks which setup steps have completed so re-runs skip finished work.
# Steps are stored in ~/.config/oqto/setup-steps-done alongside the state file.
# Use --fresh to clear completed steps and start over.
#

SETUP_STEPS_FILE="${XDG_CONFIG_HOME}/oqto/setup-steps-done"

# Check if a step has already been completed
step_done() {
  local step="$1"
  [[ -f "$SETUP_STEPS_FILE" ]] && grep -qxF "$step" "$SETUP_STEPS_FILE"
}

# Mark a step as completed
mark_step_done() {
  local step="$1"
  mkdir -p "$(dirname "$SETUP_STEPS_FILE")"
  if ! step_done "$step"; then
    echo "$step" >>"$SETUP_STEPS_FILE"
  fi
}

# Run a step if not already completed; mark done on success
# Usage: run_step "step_name" "description" command [args...]
run_step() {
  local step="$1"
  local desc="$2"
  shift 2

  if step_done "$step"; then
    log_success "Already done: $desc"
    return 0
  fi

  "$@"
  local rc=$?
  if [[ $rc -eq 0 ]]; then
    mark_step_done "$step"
  else
    log_warn "Step failed (non-fatal): $desc"
  fi
}

# Run a step unconditionally (always executes, never skipped).
# Used for steps like building where stale artifacts cause subtle bugs.
# Returns non-zero on failure so callers can decide whether to abort.
run_step_always() {
  local step="$1"
  local desc="$2"
  shift 2

  log_info "Running: $desc"
  "$@"
  local rc=$?
  if [[ $rc -eq 0 ]]; then
    mark_step_done "$step"
  else
    log_error "Step failed: $desc"
    return 1
  fi
}

# Clear all completed steps (used with --fresh)
clear_steps() {
  rm -f "$SETUP_STEPS_FILE"
}

# Load oqto.setup.toml config file and set environment variables.
# This is a simple TOML parser that handles the flat structure generated
# by the web configurator at oqto.dev/setup.
load_setup_config() {
  local config_file="$1"
  local current_section=""

  local current_provider=""

  while IFS= read -r line; do
    # Strip comments and whitespace
    line="${line%%#*}"
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"
    [[ -z "$line" ]] && continue

    # Section header
    if [[ "$line" =~ ^\[([a-z_]+)\]$ ]]; then
      current_section="${BASH_REMATCH[1]}"
      current_provider=""
      continue
    fi

    # Custom provider section: [providers.<name>]
    if [[ "$line" =~ ^\[providers\.([a-zA-Z0-9_-]+)\]$ ]]; then
      current_section="providers.custom"
      current_provider="${BASH_REMATCH[1]}"
      if [[ ! " ${CUSTOM_PROVIDERS[*]} " =~ " ${current_provider} " ]]; then
        CUSTOM_PROVIDERS+=("${current_provider}")
      fi
      continue
    fi

    # Key = value
    if [[ "$line" =~ ^([a-z_]+)[[:space:]]*=[[:space:]]*(.+)$ ]]; then
      local key="${BASH_REMATCH[1]}"
      local val="${BASH_REMATCH[2]}"

      # Strip quotes from string values
      val="${val#\"}"
      val="${val%\"}"

      # Custom provider fields
      if [[ "$current_section" == "providers.custom" && -n "$current_provider" ]]; then
        case "$key" in
          type) CP_TYPE["$current_provider"]="$val" ;;
          base_url) CP_BASE_URL["$current_provider"]="$val" ;;
          api_key) CP_API_KEY["$current_provider"]="$val" ;;
          deployment) CP_DEPLOYMENT["$current_provider"]="$val" ;;
          api_version) CP_API_VERSION["$current_provider"]="$val" ;;
          aws_region) CP_AWS_REGION["$current_provider"]="$val" ;;
          gcp_project) CP_GCP_PROJECT["$current_provider"]="$val" ;;
          gcp_location) CP_GCP_LOCATION["$current_provider"]="$val" ;;
          test_model) CP_TEST_MODEL["$current_provider"]="$val" ;;
        esac
        continue
      fi

      case "${current_section}.${key}" in
        deployment.user_mode)       OQTO_USER_MODE="$val"; SELECTED_USER_MODE="$val" ;;
        deployment.backend_mode)    OQTO_BACKEND_MODE="$val"; SELECTED_BACKEND_MODE="$val" ;;
        deployment.container_runtime) OQTO_CONTAINER_RUNTIME="$val" ;;
        deployment.workspace_dir)   WORKSPACE_DIR="$val" ;;
        network.log_level)          OQTO_LOG_LEVEL="$val" ;;
        network.caddy)              [[ "$val" == "true" ]] && SETUP_CADDY="yes" && OQTO_SETUP_CADDY="yes" ;;
        network.domain)             DOMAIN="$val"; OQTO_DOMAIN="$val" ;;
        admin.username)             ADMIN_USERNAME="$val" ;;
        admin.email)                ADMIN_EMAIL="$val" ;;
        providers.enabled)
          # Parse TOML array: ["anthropic", "openai"]
          val="${val#[}"
          val="${val%]}"
          CONFIGURED_PROVIDERS=""
          local IFS=','
          for provider in $val; do
            provider="${provider#"${provider%%[![:space:]]*}"}"
            provider="${provider%"${provider##*[![:space:]]}"}"
            provider="${provider#\"}"
            provider="${provider%\"}"
            [[ -n "$provider" ]] && CONFIGURED_PROVIDERS="${CONFIGURED_PROVIDERS} ${provider}"
          done
          CONFIGURED_PROVIDERS="${CONFIGURED_PROVIDERS# }"
          ;;
        tools.install_all)
          if [[ "$val" == "true" ]]; then
            INSTALL_ALL_TOOLS="true"
            INSTALL_MMRY="true"
            OQTO_INSTALL_AGENT_TOOLS="yes"
          fi
          ;;
        tools.searxng)              [[ "$val" == "true" ]] && INSTALL_SEARXNG="true" ;;
        hardening.enabled)
          if [[ "$val" == "true" ]]; then
            OQTO_HARDEN_SERVER="yes"
          else
            OQTO_HARDEN_SERVER="no"
          fi
          ;;
        hardening.ssh_port)         OQTO_SSH_PORT="$val" ;;
        hardening.firewall)         [[ "$val" == "true" ]] && OQTO_SETUP_FIREWALL="yes" || OQTO_SETUP_FIREWALL="no" ;;
        hardening.fail2ban)         [[ "$val" == "true" ]] && OQTO_SETUP_FAIL2BAN="yes" || OQTO_SETUP_FAIL2BAN="no" ;;
        hardening.ssh_hardening)    [[ "$val" == "true" ]] && OQTO_HARDEN_SSH="yes" || OQTO_HARDEN_SSH="no" ;;
        hardening.auto_updates)     [[ "$val" == "true" ]] && OQTO_SETUP_AUTO_UPDATES="yes" || OQTO_SETUP_AUTO_UPDATES="no" ;;
        hardening.kernel_security)  [[ "$val" == "true" ]] && OQTO_HARDEN_KERNEL="yes" || OQTO_HARDEN_KERNEL="no" ;;
      esac
    fi
  done < "$config_file"

  log_success "Config loaded: mode=${SELECTED_USER_MODE:-single}, providers=${CONFIGURED_PROVIDERS:-none}"
}

# Run a step with verification: skip only if both marked done AND verify passes
# Usage: verify_or_rerun "step_name" "description" "verify_cmd" install_func
verify_or_rerun() {
  local step="$1"
  local desc="$2"
  local verify="$3"
  local func="$4"

  if step_done "$step" && eval "$verify" &>/dev/null; then
    log_success "Already done: $desc"
    return 0
  fi

  # Clear stale marker if verify failed
  if step_done "$step"; then
    log_warn "$desc marked done but verification failed, re-running..."
    sed -i "/^${step}$/d" "$SETUP_STEPS_FILE" 2>/dev/null || true
  fi

  run_step "$step" "$desc" "$func"
}

