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

# Workspace placement (ADR-0019/0020). Distinct from the legacy backend mode
# above: placement decides where a *Workspace runner* runs, not how the backend
# hosts sessions. Container placement needs rootless Podman >= 4.0, which
# check_prerequisites installs and `oqtoctl doctor --profile container`
# verifies fail-closed.
select_placement_mode() {
  log_step "Workspace Placement Selection"

  echo
  echo "Where should Workspace runners execute?"
  echo
  echo -e "  ${BOLD}Host${NC} - Runner as a host process (default)"
  echo "    - Lowest overhead, no container runtime required"
  echo "    - Workspaces share the host filesystem and network"
  echo
  echo -e "  ${BOLD}Container${NC} - Per-Workspace rootless Podman container"
  echo "    - Tenant isolation via userns=auto, network=none by default"
  echo "    - Only granted endpoints (EAVS) are reachable from a Workspace"
  echo "    - Requires rootless Podman >= 4.0 and systemd cgroup v2"

  local choice
  choice=$(prompt_choice "Select workspace placement:" "Host" "Container")

  case "$choice" in
  "Container") SELECTED_PLACEMENT_MODE="container" ;;
  *) SELECTED_PLACEMENT_MODE="local" ;;
  esac

  log_info "Selected workspace placement: $SELECTED_PLACEMENT_MODE"
}

