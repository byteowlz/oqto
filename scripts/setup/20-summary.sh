# ==============================================================================
# Summary and Next Steps
# ==============================================================================

start_all_services() {
  log_step "Starting services"

  local is_user_service="false"
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    is_user_service="true"
  fi

  start_svc() {
    local svc="$1"
    local user_svc="${2:-false}"

    if [[ "$user_svc" == "true" ]]; then
      if systemctl --user is-active "$svc" &>/dev/null; then
        log_success "$svc: already running"
      elif systemctl --user is-enabled "$svc" &>/dev/null; then
        systemctl --user start "$svc" && log_success "$svc: started" || log_warn "$svc: failed to start"
      fi
    else
      if sudo systemctl is-active "$svc" &>/dev/null; then
        log_success "$svc: already running"
      elif sudo systemctl is-enabled "$svc" &>/dev/null; then
        sudo systemctl start "$svc" && log_success "$svc: started" || log_warn "$svc: failed to start"
      fi
    fi
  }

  # Core services (restart to pick up rebuilt binaries)
  if [[ "$is_user_service" == "false" ]]; then
    # Multi-user: restart system services to pick up new binaries
    for svc in oqto-usermgr eavs oqto; do
      if sudo systemctl is-active "$svc" &>/dev/null; then
        sudo systemctl restart "$svc" && log_success "$svc: restarted" || log_warn "$svc: failed to restart"
      elif sudo systemctl is-enabled "$svc" &>/dev/null; then
        sudo systemctl start "$svc" && log_success "$svc: started" || log_warn "$svc: failed to start"
      fi
    done

    # Re-provision all existing platform users' runner services.
    # The usermgr was just restarted with new service file templates,
    # so we need to push updated service files to all octo_* users and
    # restart their runners.
    log_info "Updating per-user services for existing platform users..."
    for user_home in /home/octo_*; do
      local username
      username=$(basename "$user_home")
      local uid
      uid=$(id -u "$username" 2>/dev/null) || continue

      log_info "Updating services for $username (uid=$uid)..."

      # Stop existing user services (they have stale service files)
      local runtime_dir="/run/user/$uid"
      local bus="unix:path=${runtime_dir}/bus"
      sudo runuser -u "$username" -- env \
        XDG_RUNTIME_DIR="$runtime_dir" \
        DBUS_SESSION_BUS_ADDRESS="$bus" \
        systemctl --user stop oqto-runner hstry mmry 2>/dev/null || true

      # Remove stale socket
      sudo rm -f "/run/oqto/runner-sockets/${username}/oqto-runner.sock"

      # Trigger usermgr to rewrite service files and restart.
      # The usermgr socket is owned by oqto:root 0600, so we must run as oqto.
      if [[ -S /run/oqto/usermgr.sock ]]; then
        local response
        response=$(sudo -u oqto python3 -c "
import socket, json, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect('/run/oqto/usermgr.sock')
req = json.dumps({'cmd': 'setup-user-runner', 'args': {'username': '${username}', 'uid': ${uid}}}) + '\n'
s.sendall(req.encode())
data = b''
while True:
    chunk = s.recv(4096)
    if not chunk: break
    data += chunk
    if b'\n' in data: break
s.close()
print(data.decode().strip())
" 2>/dev/null)
        if echo "$response" | grep -q '"ok":true'; then
          log_success "$username: services updated"
        else
          log_warn "$username: setup-user-runner failed: $response"
        fi
      fi
    done
  else
    start_svc eavs "$is_user_service"
    start_svc oqto "$is_user_service"
  fi

  # Reverse proxy
  if [[ "$SETUP_CADDY" == "yes" ]]; then
    start_svc caddy
  fi

  # Optional services
  if sudo systemctl is-enabled searxng &>/dev/null || systemctl --user is-enabled searxng &>/dev/null; then
    start_svc searxng "$is_user_service"
  fi

  # Restart oqto to pick up any config changes from this setup run
  if [[ "$is_user_service" == "true" ]]; then
    systemctl --user restart oqto &>/dev/null || true
  else
    sudo systemctl restart oqto &>/dev/null || true
  fi
  log_success "All services started"
}

print_summary() {
  log_step "Setup Complete!"

  echo
  echo "============================================================"
  echo "                    SERVICE STATUS"
  echo "============================================================"
  echo

  # Helper to check service status
  check_service_status() {
    local name="$1"
    local user_service="${2:-false}"

    if [[ "$user_service" == "true" ]]; then
      if systemctl --user is-active "$name" &>/dev/null; then
        echo -e "${GREEN}running${NC}"
      elif systemctl --user is-enabled "$name" &>/dev/null; then
        echo -e "${YELLOW}enabled (not running)${NC}"
      else
        echo -e "${RED}not configured${NC}"
      fi
    else
      if systemctl is-active "$name" &>/dev/null; then
        echo -e "${GREEN}running${NC}"
      elif systemctl is-enabled "$name" &>/dev/null; then
        echo -e "${YELLOW}enabled (not running)${NC}"
      else
        echo -e "${RED}not configured${NC}"
      fi
    fi
  }

  local is_user_service="false"
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    is_user_service="true"
  fi

  echo -e "  EAVS (LLM):     $(check_service_status eavs "$is_user_service")"
  echo -e "  Oqto backend:   $(check_service_status oqto "$is_user_service")"

  if [[ "$SETUP_CADDY" == "yes" ]]; then
    echo -e "  Caddy:          $(check_service_status caddy)"
  fi

  echo -e "  SearXNG:        $(check_service_status searxng "$is_user_service")"

  if command_exists valkey-server; then
    echo -e "  Valkey:         $(check_service_status valkey)"
  elif command_exists redis-server; then
    echo -e "  Redis:          $(check_service_status redis)"
  fi

  if [[ "$OS" == "linux" ]]; then
    if [[ "$SELECTED_USER_MODE" == "multi" ]]; then
      echo -e "  hstry:          ${CYAN}per-user (managed by runner)${NC}"
    else
      echo -e "  hstry:          $(check_service_status hstry "$is_user_service")"
    fi
  fi

  echo
  echo "============================================================"
  echo "                    CONFIGURATION"
  echo "============================================================"
  echo
  echo "  User mode:       $SELECTED_USER_MODE"
  echo "  Backend mode:    $SELECTED_BACKEND_MODE"
  echo "  Deployment mode: $([[ "$PRODUCTION_MODE" == "true" ]] && echo "Production" || echo "Development")"
  echo "  Config file:     $OQTO_CONFIG_DIR/config.toml"
  echo

  if [[ "$PRODUCTION_MODE" == "true" ]]; then
    echo "  Security:"
    echo "    JWT secret:    configured (64 characters)"
    echo "    Admin user:    $ADMIN_USERNAME"
    echo "    Admin email:   $ADMIN_EMAIL"
    if [[ "$NONINTERACTIVE" == "true" ]]; then
      echo -e "    ${YELLOW}Admin password was shown during setup${NC}"
    fi
    echo

    if [[ "$SETUP_CADDY" == "yes" ]]; then
      echo "  Reverse Proxy:"
      echo "    Caddy:         installed"
      echo "    Domain:        $DOMAIN"
      if [[ "$DOMAIN" != "localhost" ]]; then
        echo "    HTTPS:         enabled (automatic via Let's Encrypt)"
      fi
      echo "    Caddyfile:     /etc/caddy/Caddyfile"
      echo
    fi

    if [[ "$OQTO_HARDEN_SERVER" == "yes" && "$OS" == "linux" ]]; then
      echo "  Server Hardening:"
      echo "    Firewall:      $(command_exists ufw && echo 'UFW enabled' || (command_exists firewall-cmd && echo 'firewalld enabled' || echo 'not configured'))"
      echo -e "    Fail2ban:      $(check_service_status fail2ban)"
      echo "    SSH port:      ${OQTO_SSH_PORT:-22}"
      echo "    SSH auth:      public key only (password disabled)"
      echo "    Auto updates:  ${OQTO_SETUP_AUTO_UPDATES}"
      echo "    Kernel:        hardened sysctl parameters"
      echo -e "    Audit:         $(check_service_status auditd)"
      echo
    fi
  fi

  local eavs_cfg
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_cfg="${XDG_CONFIG_HOME}/eavs/config.toml"
  else
    eavs_cfg="/etc/eavs/config.toml"
  fi

  echo "  LLM Access (EAVS):"
  echo "    Proxy URL:     http://127.0.0.1:${EAVS_PORT}"
  echo "    Config:        $eavs_cfg"
  if [[ -f "$eavs_cfg" ]]; then
    local configured_providers
    configured_providers=$(grep '^\[providers\.' "$eavs_cfg" 2>/dev/null | sed 's/\[providers\.\(.*\)\]/\1/' | grep -v '^default$' | tr '\n' ', ' | sed 's/,$//')
    if [[ -n "$configured_providers" ]]; then
      echo -e "    Providers:     ${GREEN}${configured_providers}${NC}"
    else
      echo -e "    Providers:     ${RED}none configured${NC}"
    fi
  else
    echo -e "    Providers:     ${YELLOW}config not found${NC}"
  fi

  echo
  echo "============================================================"
  echo "                    INSTALLED SOFTWARE"
  echo "============================================================"
  echo

  # Helper: check if binary exists and show path or red "missing"
  check_bin() {
    local name="$1"
    local path
    path=$(which "$name" 2>/dev/null)
    if [[ -n "$path" ]]; then
      echo -e "${GREEN}$path${NC}"
    else
      echo -e "${RED}missing${NC}"
    fi
  }

  echo "  Core binaries:"
  echo -e "    oqto:          $(check_bin oqto)"
  echo -e "    eavs:          $(check_bin eavs)"
  echo -e "    oqto-files:    $(check_bin oqto-files)"
  echo -e "    pi:            $(check_bin pi)"
  if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
    echo -e "    ttyd:          $(check_bin ttyd)"
  fi
  if [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
    echo -e "    oqto-runner:   $(check_bin oqto-runner)"
  fi
  if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
    echo -e "    pi-bridge:     $(check_bin pi-bridge)"
  fi
  echo

  echo "  Agent tools:"
  for tool in agntz mmry scrpr sx tmpltr sldr ignr typst slidev; do
    printf "    %-14s " "$tool:"
    echo -e "$(check_bin "$tool")"
  done
  echo

  echo "  Shell tools:"
  for tool in tmux fd rg yazi zsh zoxide; do
    printf "    %-14s " "$tool:"
    echo -e "$(check_bin "$tool")"
  done
  echo

  echo "  Pi extensions:"
  local pi_ext_dir="$HOME/.pi/agent/extensions"
  for ext_name in "${PI_DEFAULT_EXTENSIONS[@]}"; do
    printf "    %-22s " "${ext_name}:"
    if [[ -d "${pi_ext_dir}/${ext_name}" ]]; then
      echo -e "${GREEN}installed${NC}"
    else
      echo -e "${RED}missing${NC}"
    fi
  done
  echo

  echo "============================================================"
  echo "                    NEXT STEPS"
  echo "============================================================"
  echo

  local step=1

  # Check which services need starting
  local need_start=()

  # Helper: check if a service needs starting
  service_needs_start() {
    local svc="$1"
    if [[ "$SELECTED_USER_MODE" == "single" ]]; then
      ! systemctl --user is-active "$svc" &>/dev/null
    else
      ! systemctl is-active "$svc" &>/dev/null
    fi
  }

  if [[ "$OS" == "linux" ]]; then
    service_needs_start eavs && need_start+=("eavs")
    service_needs_start oqto && need_start+=("oqto")
    if [[ "$SETUP_CADDY" == "yes" ]]; then
      service_needs_start caddy && need_start+=("caddy")
    fi
    if systemctl --user is-enabled searxng &>/dev/null || systemctl is-enabled searxng &>/dev/null; then
      service_needs_start searxng && need_start+=("searxng")
    fi
  fi

  if [[ ${#need_start[@]} -gt 0 ]]; then
    echo "  $step. Start services that are not yet running:"
    for svc in "${need_start[@]}"; do
      if [[ "$SELECTED_USER_MODE" == "single" ]]; then
        echo "     systemctl --user start $svc"
      else
        echo "     sudo systemctl start $svc"
      fi
    done
    echo
    ((step++))
  fi

  if [[ "$PRODUCTION_MODE" == "true" ]]; then
    echo "  $step. Access the web interface:"
    if [[ -n "$DOMAIN" && "$DOMAIN" != "localhost" ]]; then
      echo "     https://${DOMAIN}"
    else
      echo "     http://localhost:3000"
    fi
    echo
    ((step++))

    echo "  $step. Login with admin credentials:"
    echo "     Username: $ADMIN_USERNAME"
    echo "     Password: (the password you entered during setup)"
    echo
    ((step++))

    echo "  $step. Create invite codes for new users:"
    echo "     oqtoctl invites create --uses 1"
    echo "     # Or use the admin interface"
    echo
  else
    echo "  $step. Start the frontend dev server:"
    echo "     cd $SCRIPT_DIR/frontend && bun dev"
    echo
    ((step++))

    echo "  $step. Open the web interface:"
    echo "     http://localhost:3000"
    echo
    ((step++))

    if [[ "$OQTO_DEV_MODE" == "true" && -n "${dev_user_id:-}" ]]; then
      echo "  $step. Login with your dev credentials:"
      echo "     Username: $dev_user_id"
      echo "     Password: (the password you entered)"
      echo
      ((step++))
    fi
  fi

  # Show API key warning if not configured
  if [[ "$EAVS_ENABLED" != "true" && "$LLM_API_KEY_SET" != "true" && -n "$LLM_PROVIDER" ]]; then
    echo -e "  ${YELLOW}IMPORTANT:${NC} Set your API key before starting Oqto:"
    case "$LLM_PROVIDER" in
    anthropic)
      echo "     export ANTHROPIC_API_KEY=your-key-here"
      ;;
    openai)
      echo "     export OPENAI_API_KEY=your-key-here"
      ;;
    openrouter)
      echo "     export OPENROUTER_API_KEY=your-key-here"
      ;;
    google)
      echo "     export GOOGLE_API_KEY=your-key-here"
      ;;
    groq)
      echo "     export GROQ_API_KEY=your-key-here"
      ;;
    esac
    echo
  fi

  # macOS note about env file
  if [[ "$OS" == "macos" && "$LLM_API_KEY_SET" == "true" ]]; then
    echo "  Note: On macOS, source the env file before starting manually:"
    echo "     source $OQTO_CONFIG_DIR/env"
    echo
  fi

  echo "For more information, see:"
  echo "  - README.md"
  echo "  - SETUP.md (detailed setup guide)"
  echo "  - deploy/systemd/README.md (Linux systemd setup)"
  echo "  - deploy/ansible/README.md (Ansible deployment)"
  echo "  - backend/examples/config.toml (full config reference)"
}

