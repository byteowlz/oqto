# ==============================================================================
# Mode Selection
# ==============================================================================

select_user_mode() {
  log_step "User Mode Selection"

  echo
  echo "Oqto supports two user modes:"
  echo
  echo -e "  ${BOLD}Multi-user${NC} - Team deployment (default)"
  echo "    - Each user gets an isolated workspace"
  echo "    - User authentication and management"
  echo "    - Best for: teams, shared servers"
  echo
  echo -e "  ${BOLD}Single-user${NC} - Personal deployment"
  echo "    - All sessions use the same workspace"
  echo "    - Simpler setup, no user management"
  echo "    - Best for: personal laptops, single-developer servers"

  if [[ "$OS" == "macos" ]]; then
    echo
    echo -e "  ${YELLOW}Note: Multi-user on macOS requires Docker/Podman${NC}"
  fi

  local choice
  choice=$(prompt_choice "Select user mode:" "Multi-user" "Single-user")

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

  if [[ "${SELECTED_BACKEND_MODE:-}" == "container" ]]; then
    log_warn "Container backend is temporarily disabled; forcing local backend mode"
  fi

  echo
  echo "Oqto currently supports backend mode:"
  echo
  echo -e "  ${BOLD}Local${NC} - Native processes"
  echo "    - Runs Pi, oqto-files, ttyd directly on host"
  echo "    - Lower overhead, faster startup"
  echo "    - Best for: development and current production path"
  echo
  echo -e "  ${YELLOW}Container mode is temporarily disabled until fully finished and tested.${NC}"

  SELECTED_BACKEND_MODE="local"
  log_info "Selected backend mode: $SELECTED_BACKEND_MODE"
}

