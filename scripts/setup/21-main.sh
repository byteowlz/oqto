# ==============================================================================
# Main
# ==============================================================================

show_help() {
  cat <<EOF
________  ________  _________  ________
|\   __  \|\   __  \|\___   ___\\   __  \
\ \  \|\  \ \  \|\  \|___ \  \_\ \  \|\  \
 \ \  \\\  \ \  \\\  \   \ \  \ \ \  \\\  \
  \ \  \\\  \ \  \\\  \   \ \  \ \ \  \\\  \
   \ \_______\ \_____  \   \ \__\ \ \_______\
    \|_______|\|___| \__\   \|__|  \|_______|
                    \|__|

            got tentacles?

Oqto Setup Script

Usage: $0 [OPTIONS]

Options:
  --help                Show this help message
  --non-interactive     Run without prompts (uses defaults/env vars)
  
  --production, --prod  Production mode with ALL hardening enabled:
                        - Disables dev mode (requires real auth)
                        - Enables firewall, fail2ban, SSH hardening
                        - Enables auto-updates and kernel hardening
                        - Installs all dependencies and services
  
  --dev, --development  Development mode (no hardening, dev auth enabled)
  
  --domain <domain>     Set domain and enable Caddy reverse proxy
                        Example: --domain oqto.example.com
  
  --from-url <url>      Load config from an oqto.dev/setup URL.
                        The URL fragment contains the config as base64.
                        Example: --from-url 'https://oqto.dev/setup#WyRzY2...'
  
  --ssh-port <port>     Set SSH port for hardening (default: 22)
  
  Disable specific hardening features (use with --production):
  --no-firewall         Skip firewall configuration
  --no-fail2ban         Skip fail2ban installation
  --no-ssh-hardening    Skip SSH hardening (keeps password auth)
  --no-auto-updates     Skip automatic security updates
  --no-kernel-hardening Skip kernel sysctl hardening
  
  State management:
  --update              Pull latest code, rebuild, deploy, and restart services
  --fresh               Clear all saved state and completed steps, start over
  --redo step1,step2    Re-run specific steps (comma-separated)
                        (state: ~/.config/oqto/setup-state.env)
                        (steps: ~/.config/oqto/setup-steps-done)

  Tool installation:
  --all-tools           Install all byteowlz agent tools
  --no-agent-tools      Skip agent tools installation

Environment Variables:
  OQTO_USER_MODE          single or multi (default: single)
  OQTO_BACKEND_MODE       local or container (default: local)
  OQTO_CONTAINER_RUNTIME  docker, podman, or auto (default: auto)
  OQTO_INSTALL_DEPS       yes or no (default: yes)
  OQTO_INSTALL_SERVICE    yes or no (default: yes)
  OQTO_INSTALL_AGENT_TOOLS yes or no (default: yes)
  OQTO_DEV_MODE           true or false (default: prompt user)
  OQTO_LOG_LEVEL          error, warn, info, debug, trace (default: info)
  OQTO_SETUP_CADDY        yes or no (default: prompt user in production mode)
  OQTO_DOMAIN             domain for HTTPS (e.g., oqto.example.com)

Server Hardening (Linux production mode only):
  OQTO_HARDEN_SERVER      yes or no (default: prompt in production mode)
  OQTO_SSH_PORT           SSH port number (default: 22)
  OQTO_SETUP_FIREWALL     yes or no - configure UFW/firewalld (default: yes)
  OQTO_SETUP_FAIL2BAN     yes or no - install and configure fail2ban (default: yes)
  OQTO_HARDEN_SSH         yes or no - apply SSH hardening (default: yes)
  OQTO_SETUP_AUTO_UPDATES yes or no - enable automatic security updates (default: yes)
  OQTO_HARDEN_KERNEL      yes or no - apply kernel security parameters (default: yes)

LLM Provider API Keys (set one of these, or use EAVS):
  ANTHROPIC_API_KEY       Anthropic Claude API key
  OPENAI_API_KEY          OpenAI API key
  OPENROUTER_API_KEY      OpenRouter API key
  GOOGLE_API_KEY          Google AI API key
  GROQ_API_KEY            Groq API key

Shell Tools Installed:
  tmux, fd, ripgrep, yazi, zsh, zoxide

Agent Tools:
  agntz   - Agent toolkit (file reservations, tool management)
  mmry    - Memory storage and semantic search
  scrpr   - Web content extraction (readability, Tavily, Jina)
  sx      - Web search via local SearXNG instance
  tmpltr  - Document generation from templates (Typst)
  ignr    - Gitignore generation (auto-detect)

Other Tools:
  ttyd    - Web terminal
  pi      - Main chat interface (primary agent harness)

Search Engine:
  SearXNG - Local privacy-respecting metasearch engine (for sx)
  Valkey  - In-memory cache for SearXNG rate limiting

Pi Extensions (from github.com/byteowlz/pi-agent-extensions):
  auto-rename          - Auto-generate session names from first query
  oqto-bridge          - Emit agent phase status for the Oqto runner
  oqto-todos           - Todo management for Oqto UI
  custom-context-files - Auto-load USER.md, PERSONALITY.md into prompts

For detailed documentation on all prerequisites and components, see SETUP.md

Examples:
  # Interactive setup (recommended for first-time)
  ./setup.sh

  # Quick development setup (no prompts)
  ./setup.sh --dev

  # Full production setup with all hardening (RECOMMENDED for servers)
  ./setup.sh --production --domain oqto.example.com

  # Production with custom SSH port
  ./setup.sh --production --domain oqto.example.com --ssh-port 2222

  # Production but keep password SSH auth (for initial setup)
  ./setup.sh --production --domain oqto.example.com --no-ssh-hardening

  # Deploy from a pre-configured oqto.dev/setup URL
  ./setup.sh --from-url 'https://oqto.dev/setup#WyRzY2hlbWEi...'

  # Multi-user container setup on Linux
  OQTO_USER_MODE=multi OQTO_BACKEND_MODE=container ./setup.sh --production

  # Environment variable style (equivalent to --production)
  OQTO_DEV_MODE=false OQTO_HARDEN_SERVER=yes ./setup.sh --non-interactive
EOF
}

main() {
  NONINTERACTIVE="false"
  FRESH_SETUP="false"

  # Parse arguments
  while [[ $# -gt 0 ]]; do
    case "$1" in
    --help | -h)
      show_help
      exit 0
      ;;
    --update)
      UPDATE_MODE="true"
      shift
      ;;
    --fresh)
      FRESH_SETUP="true"
      rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/oqto/setup-state.env"
      rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/oqto/setup-steps-done"
      shift
      ;;
    --redo)
      # Clear specific steps so they re-run: --redo step1,step2,...
      local redo_steps="${2:-}"
      if [[ -z "$redo_steps" ]]; then
        log_error "--redo requires comma-separated step names"
        log_info "Steps file: ${XDG_CONFIG_HOME:-$HOME/.config}/oqto/setup-steps-done"
        exit 1
      fi
      local steps_file="${XDG_CONFIG_HOME:-$HOME/.config}/oqto/setup-steps-done"
      if [[ -f "$steps_file" ]]; then
        IFS=',' read -ra steps_to_redo <<<"$redo_steps"
        for s in "${steps_to_redo[@]}"; do
          sed -i "/^${s}$/d" "$steps_file" 2>/dev/null || true
          log_info "Cleared step: $s"
        done
      fi
      shift 2
      ;;
    --non-interactive)
      NONINTERACTIVE="true"
      shift
      ;;
    --production | --prod)
      # Production mode with all hardening enabled
      NONINTERACTIVE="true"
      OQTO_DEV_MODE="false"
      OQTO_HARDEN_SERVER="yes"
      OQTO_SETUP_FIREWALL="yes"
      OQTO_SETUP_FAIL2BAN="yes"
      OQTO_HARDEN_SSH="yes"
      OQTO_SETUP_AUTO_UPDATES="yes"
      OQTO_HARDEN_KERNEL="yes"
      OQTO_INSTALL_DEPS="yes"
      OQTO_INSTALL_SERVICE="yes"
      OQTO_INSTALL_AGENT_TOOLS="yes"
      shift
      ;;
    --dev | --development)
      # Development mode, no hardening
      NONINTERACTIVE="true"
      OQTO_DEV_MODE="true"
      OQTO_HARDEN_SERVER="no"
      shift
      ;;
    --domain)
      OQTO_DOMAIN="$2"
      OQTO_SETUP_CADDY="yes"
      shift 2
      ;;
    --domain=*)
      OQTO_DOMAIN="${1#*=}"
      OQTO_SETUP_CADDY="yes"
      shift
      ;;
    --ssh-port)
      OQTO_SSH_PORT="$2"
      shift 2
      ;;
    --ssh-port=*)
      OQTO_SSH_PORT="${1#*=}"
      shift
      ;;
    --no-firewall)
      OQTO_SETUP_FIREWALL="no"
      shift
      ;;
    --no-fail2ban)
      OQTO_SETUP_FAIL2BAN="no"
      shift
      ;;
    --no-ssh-hardening)
      OQTO_HARDEN_SSH="no"
      shift
      ;;
    --no-auto-updates)
      OQTO_SETUP_AUTO_UPDATES="no"
      shift
      ;;
    --no-kernel-hardening)
      OQTO_HARDEN_KERNEL="no"
      shift
      ;;
    --all-tools)
      INSTALL_ALL_TOOLS="true"
      INSTALL_MMRY="true"
      shift
      ;;
    --no-agent-tools)
      OQTO_INSTALL_AGENT_TOOLS="no"
      shift
      ;;
    --config)
      SETUP_CONFIG_FILE="$2"
      shift 2
      ;;
    --config=*)
      SETUP_CONFIG_FILE="${1#*=}"
      shift
      ;;
    --from-url)
      SETUP_FROM_URL="$2"
      shift 2
      ;;
    --from-url=*)
      SETUP_FROM_URL="${1#*=}"
      shift
      ;;
    *)
      log_error "Unknown option: $1"
      show_help
      exit 1
      ;;
    esac
  done

  # Decode setup URL if specified (oqto.dev/setup#...)
  if [[ -n "${SETUP_FROM_URL:-}" ]]; then
    local url_config_file
    url_config_file="$(mktemp /tmp/oqto-setup-XXXXXX.toml)"
    if ! decode_setup_url "$SETUP_FROM_URL" "$url_config_file"; then
      rm -f "$url_config_file"
      exit 1
    fi
    SETUP_CONFIG_FILE="$url_config_file"
  fi

  # Load config file if specified (oqto.setup.toml)
  if [[ -n "${SETUP_CONFIG_FILE:-}" ]]; then
    if [[ ! -f "$SETUP_CONFIG_FILE" ]]; then
      log_error "Config file not found: $SETUP_CONFIG_FILE"
      exit 1
    fi
    log_info "Loading config from: $SETUP_CONFIG_FILE"
    load_setup_config "$SETUP_CONFIG_FILE"
    NONINTERACTIVE="true"
  fi

  # Apply env vars from web configurator (oqto.dev/setup deploy command)
  if [[ -n "$OQTO_PROVIDERS" ]]; then
    CONFIGURED_PROVIDERS="${OQTO_PROVIDERS//,/ }"
  fi
  if [[ "$OQTO_INSTALL_ALL_TOOLS" == "yes" ]]; then
    INSTALL_ALL_TOOLS="true"
    INSTALL_MMRY="true"
  fi
  if [[ -n "$OQTO_WORKSPACE_DIR" ]]; then
    WORKSPACE_DIR="$OQTO_WORKSPACE_DIR"
  fi
  if [[ -n "$OQTO_ADMIN_USER" ]]; then
    ADMIN_USERNAME="$OQTO_ADMIN_USER"
  fi
  if [[ -n "$OQTO_ADMIN_EMAIL" ]]; then
    ADMIN_EMAIL="$OQTO_ADMIN_EMAIL"
  fi
  # Map OQTO_USER_MODE to internal variable
  if [[ -n "$OQTO_USER_MODE" ]]; then
    SELECTED_USER_MODE="$OQTO_USER_MODE"
  fi
  if [[ -n "$OQTO_BACKEND_MODE" ]]; then
    SELECTED_BACKEND_MODE="$OQTO_BACKEND_MODE"
  fi
  if [[ -n "$OQTO_SETUP_CADDY" && "$OQTO_SETUP_CADDY" == "yes" ]]; then
    SETUP_CADDY="yes"
    if [[ -n "$OQTO_DOMAIN" ]]; then
      DOMAIN="$OQTO_DOMAIN"
    fi
  fi

  echo
  echo -e "${BOLD}${CYAN}"
  cat <<'BANNER'
 ________  ________  _________  ________
|\   __  \|\   __  \|\___   ___\\   __  \
\ \  \|\  \ \  \|\  \|___ \  \_\ \  \|\  \
 \ \  \\\  \ \  \\\  \   \ \  \ \ \  \\\  \
  \ \  \\\  \ \  \\\  \   \ \  \ \ \  \\\  \
   \ \_______\ \_____  \   \ \__\ \ \_______\
    \|_______|\|___| \__\   \|__|  \|_______|
                    \|__|
BANNER
  echo -e "${NC}"
  echo -e "${BOLD}            got tentacles?${NC}"
  echo

  # Save state on exit (including failures) so re-runs can pick up where we left off
  trap save_setup_state EXIT

  # Initialize
  detect_os

  # Update mode: just pull, rebuild, deploy, restart
  if [[ "${UPDATE_MODE:-}" == "true" ]]; then
    update_octo
    return 0
  fi

  # Load previous setup state (if available and not --fresh)
  local use_saved_state="false"
  if [[ "$FRESH_SETUP" != "true" && "$NONINTERACTIVE" != "true" ]]; then
    if load_setup_state; then
      if confirm "Reuse previous setup decisions?"; then
        apply_setup_state
        use_saved_state="true"
      fi
    fi
  elif [[ "$FRESH_SETUP" != "true" && "$NONINTERACTIVE" == "true" ]]; then
    # Non-interactive mode: silently load saved state as defaults
    if [[ -f "$SETUP_STATE_FILE" ]]; then
      apply_setup_state
      use_saved_state="true"
    fi
  fi

  # Mode selection (skip if loaded from state)
  if [[ "$use_saved_state" != "true" ]]; then
    SELECTED_USER_MODE="${OQTO_USER_MODE}"
    SELECTED_BACKEND_MODE="${OQTO_BACKEND_MODE}"
  fi

  # Ensure defaults if state did not include these
  SELECTED_USER_MODE="${SELECTED_USER_MODE:-$OQTO_USER_MODE}"
  SELECTED_BACKEND_MODE="${SELECTED_BACKEND_MODE:-$OQTO_BACKEND_MODE}"

  if [[ "$use_saved_state" != "true" ]]; then
    if [[ "$NONINTERACTIVE" != "true" ]]; then
      select_user_mode
      select_backend_mode
      select_deployment_mode
    else
      # Non-interactive: use env var or default to dev mode
      if [[ -z "$OQTO_DEV_MODE" ]]; then
        OQTO_DEV_MODE="true"
      fi
      PRODUCTION_MODE="$([[ "$OQTO_DEV_MODE" == "false" ]] && echo "true" || echo "false")"
    fi
  fi

  # Prerequisites
  check_prerequisites

  # In multi-user mode, create the oqto system user early
  if [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
    run_step "octo_system_user" "Oqto system user" ensure_octo_system_user
  fi

  # Install dependencies
  if [[ "$OQTO_INSTALL_DEPS" == "yes" ]]; then
    # Shell tools - verify all expected binaries exist, not just that the step ran
    verify_or_rerun "shell_tools" "Shell tools" \
      "command -v tmux && command -v rg && (command -v fd || command -v fdfind) && command -v yazi && command -v zoxide" \
      install_shell_tools

    if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
      verify_or_rerun "ttyd" "ttyd" "command -v ttyd" install_ttyd
    fi

    # Pi extensions - verify they're actually on disk
    verify_or_rerun "pi_extensions" "Pi extensions" \
      "test -d $HOME/.pi/agent/extensions/oqto-bridge" \
      "$(if [[ "$SELECTED_USER_MODE" == "multi" ]]; then echo install_pi_extensions_all_users; else echo install_pi_extensions; fi)"

    # Agent tools
    if [[ "$OQTO_INSTALL_AGENT_TOOLS" == "yes" ]]; then
      verify_or_rerun "agntz" "agntz" "command -v agntz" install_agntz

      if [[ "$NONINTERACTIVE" != "true" ]] && ! step_done "agent_tools_selected"; then
        select_agent_tools
        mark_step_done "agent_tools_selected"
      fi

      if [[ "$INSTALL_MMRY" == "true" || "$INSTALL_ALL_TOOLS" == "true" ]]; then
        verify_or_rerun "agent_tools" "Agent tools" \
          "command -v mmry && command -v scrpr && command -v sx && command -v tmpltr && command -v sldr && command -v ignr" \
          install_agent_tools_selected
      fi

      if [[ "$INSTALL_ALL_TOOLS" == "true" ]] || command_exists sx; then
        if ! step_done "searxng"; then
          if confirm "Install SearXNG local search engine for sx?"; then
            install_searxng
            mark_step_done "searxng"
          else
            mark_step_done "searxng"
          fi
        else
          log_success "Already done: SearXNG"
        fi
      fi
    fi
  fi

  # Upgrade all installed tools to versions in dependencies.toml.
  # verify_or_rerun only checks if a binary exists, not its version.
  # This ensures re-runs always pick up version bumps (eavs, hstry, agntz, etc.).
  update_tools

  # EAVS (LLM proxy)
  verify_or_rerun "eavs_install" "EAVS install" "command -v eavs" install_eavs
  # Always verify eavs config has [keys] enabled and master key exists.
  # A stale config from a prior install may lack virtual key support.
  local _eavs_cfg_dir
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    _eavs_cfg_dir="${XDG_CONFIG_HOME}/eavs"
  else
    _eavs_cfg_dir="${OQTO_HOME}/.config/eavs"
  fi
  verify_or_rerun "eavs_configure" "EAVS configure" \
    "grep -q '^enabled = true' '${_eavs_cfg_dir}/config.toml' 2>/dev/null && grep -q 'require_key = true' '${_eavs_cfg_dir}/config.toml' 2>/dev/null && test -f '${_eavs_cfg_dir}/env'" \
    configure_eavs
  verify_or_rerun "eavs_service" "EAVS service" "systemctl is-enabled eavs 2>/dev/null" install_eavs_service

  # Test providers and generate models.json (after eavs service is running)
  if [[ -n "${CONFIGURED_PROVIDERS:-}" ]]; then
    run_step "eavs_test" "EAVS provider tests" test_eavs_providers
  fi
  run_step "eavs_models" "EAVS models.json" generate_eavs_models_json

  # Build Oqto - ALWAYS rebuild to ensure binaries match the current source.
  # This is critical: stale binaries cause subtle bugs that are hard to diagnose.
  # The build is incremental (cargo only recompiles changed crates) so it's fast
  # when nothing changed.
  run_step_always "build_octo" "Build Oqto" build_octo || {
    log_error "Build failed. Cannot continue without binaries."
    exit 1
  }

  # Generate configuration
  run_step "generate_config" "Configuration" generate_config

  # Onboarding templates and shared repos
  run_step "onboarding_templates" "Onboarding templates" setup_onboarding_templates_repo
  run_step "external_repos" "External repos" update_external_repos
  run_step "feedback_dirs" "Feedback directories" setup_feedback_dirs

  # Linux user isolation
  verify_or_rerun "linux_user_isolation" "Linux user isolation" "test -f /etc/sudoers.d/oqto-multiuser" setup_linux_user_isolation

  # Container image
  run_step "container_image" "Container image" build_container_image

  # Caddy reverse proxy
  if [[ "$SETUP_CADDY" == "yes" ]]; then
    verify_or_rerun "caddy_install" "Caddy install" "command -v caddy" install_caddy
    # Verify Caddyfile contains our config, not the default
    verify_or_rerun "caddy_config" "Caddy config" \
      "grep -q 'reverse_proxy' /etc/caddy/Caddyfile 2>/dev/null" \
      generate_caddyfile
    verify_or_rerun "caddy_service" "Caddy service" "systemctl is-enabled caddy 2>/dev/null" install_caddy_service
  fi

  # Server hardening
  run_step "harden_server" "Server hardening" harden_server

  # Install service
  run_step_always "install_service" "System service" install_service || {
    log_error "Service installation failed. Cannot start services."
    exit 1
  }

  # Create admin user in database (always verify -- the user may have been
  # lost if the DB was recreated, even though the step ran before)
  run_step_always "admin_user_db" "Admin user in database" create_admin_user_db

  # Start all services
  start_all_services

  # Summary
  print_summary
}
