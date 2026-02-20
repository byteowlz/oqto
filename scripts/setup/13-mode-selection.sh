# ==============================================================================
# Mode Selection
# ==============================================================================

select_user_mode() {
  log_step "User Mode Selection"

  echo
  echo "Oqto supports two user modes:"
  echo
  echo -e "  ${BOLD}Single-user${NC} - Personal deployment"
  echo "    - All sessions use the same workspace"
  echo "    - Simpler setup, no user management"
  echo "    - Best for: personal laptops, single-developer servers"
  echo
  echo -e "  ${BOLD}Multi-user${NC} - Team deployment"
  echo "    - Each user gets isolated workspace"
  echo "    - User authentication and management"
  echo "    - Best for: teams, shared servers"

  if [[ "$OS" == "macos" ]]; then
    echo
    echo -e "  ${YELLOW}Note: Multi-user on macOS requires Docker/Podman${NC}"
  fi

  local choice
  choice=$(prompt_choice "Select user mode:" "Single-user" "Multi-user")

  case "$choice" in
  "Single-user")
    SELECTED_USER_MODE="single"
    ;;
  "Multi-user")
    SELECTED_USER_MODE="multi"
    # macOS multi-user requires container mode
    if [[ "$OS" == "macos" ]]; then
      log_info "Multi-user on macOS requires container mode"
      SELECTED_BACKEND_MODE="container"
    fi
    ;;
  esac

  log_info "Selected user mode: $SELECTED_USER_MODE"
}

select_backend_mode() {
  log_step "Backend Mode Selection"

  # If already set (e.g., macOS multi-user), skip
  if [[ -n "${SELECTED_BACKEND_MODE:-}" ]]; then
    log_info "Backend mode pre-selected: $SELECTED_BACKEND_MODE"
    return
  fi

  echo
  echo "Oqto can run agents in two modes:"
  echo
  echo -e "  ${BOLD}Local${NC} - Native processes"
  echo "    - Runs Pi, oqto-files, ttyd directly on host"
  echo "    - Lower overhead, faster startup"
  echo "    - Best for: development, single-user, trusted environments"
  echo
  echo -e "  ${BOLD}Container${NC} - Docker/Podman containers"
  echo "    - Full isolation per session"
  echo "    - Reproducible environment"
  echo "    - Best for: multi-user, production, untrusted code"

  local choice
  choice=$(prompt_choice "Select backend mode:" "Local" "Container")

  case "$choice" in
  "Local")
    SELECTED_BACKEND_MODE="local"
    ;;
  "Container")
    SELECTED_BACKEND_MODE="container"
    ;;
  esac

  log_info "Selected backend mode: $SELECTED_BACKEND_MODE"
}

