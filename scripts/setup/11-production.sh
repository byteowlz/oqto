# ==============================================================================
# Production Mode Setup
# ==============================================================================

select_deployment_mode() {
  log_step "Deployment Mode Selection"

  echo
  echo "Oqto can be deployed in two modes:"
  echo
  echo -e "  ${BOLD}Development${NC} - For local development and testing"
  echo "    - Uses dev_mode authentication (no JWT secret required)"
  echo "    - Preconfigured dev users for easy login"
  echo "    - HTTP only (no TLS)"
  echo "    - Best for: local development, testing"
  echo
  echo -e "  ${BOLD}Production${NC} - For server deployments"
  echo "    - Secure JWT-based authentication"
  echo "    - Creates an admin user with secure credentials"
  echo "    - Optional Caddy reverse proxy with automatic HTTPS"
  echo "    - Best for: servers, multi-user deployments, remote access"
  echo

  local choice
  choice=$(prompt_choice "Select deployment mode:" "Development" "Production")

  case "$choice" in
  "Development")
    PRODUCTION_MODE="false"
    OQTO_DEV_MODE="true"
    log_info "Development mode selected"
    ;;
  "Production")
    PRODUCTION_MODE="true"
    OQTO_DEV_MODE="false"
    log_info "Production mode selected"
    setup_production_mode
    ;;
  esac
}

setup_production_mode() {
  log_step "Production Mode Configuration"

  # Generate JWT secret (reuse saved one if available)
  echo
  if [[ -n "${JWT_SECRET:-}" ]]; then
    log_info "Using saved JWT secret"
  else
    log_info "Generating secure JWT secret..."
    JWT_SECRET=$(generate_secure_secret 64)
    log_success "JWT secret generated (64 characters)"
  fi

  # Admin user setup
  setup_admin_user

  # Caddy setup
  setup_caddy_prompt
}

generate_secure_secret() {
  local length="${1:-64}"
  # Use openssl for cryptographically secure random bytes
  if command_exists openssl; then
    openssl rand -base64 "$((length * 3 / 4))" | tr -d '/+=' | head -c "$length"
  else
    # Fallback to /dev/urandom
    head -c "$((length * 2))" /dev/urandom | base64 | tr -d '/+=' | head -c "$length"
  fi
}

setup_admin_user() {
  log_step "Admin User Setup"

  echo
  echo "Create an administrator account to manage Oqto."
  echo "This user will be able to:"
  echo "  - Access the admin dashboard"
  echo "  - Create invite codes for new users"
  echo "  - Manage sessions and users"
  echo

  # Username
  ADMIN_USERNAME=$(prompt_input "Admin username" "${ADMIN_USERNAME:-admin}")

  # Email
  ADMIN_EMAIL=$(prompt_input "Admin email" "${ADMIN_EMAIL:-admin@localhost}")

  # Password is prompted later in create_admin_user_db when actually needed.
  # We never persist plaintext passwords to disk.

  log_success "Admin user configured: $ADMIN_USERNAME"
}

setup_caddy_prompt() {
  log_step "Reverse Proxy Setup (Caddy)"

  echo
  echo "Caddy provides a reverse proxy with automatic HTTPS."
  echo "This is recommended for production deployments."
  echo
  echo "Features:"
  echo "  - Automatic TLS certificate from Let's Encrypt"
  echo "  - HTTP/2 support"
  echo "  - Simple configuration"
  echo

  if [[ -n "$OQTO_SETUP_CADDY" ]]; then
    SETUP_CADDY="$OQTO_SETUP_CADDY"
  elif confirm "Set up Caddy reverse proxy?" "y"; then
    SETUP_CADDY="yes"
  else
    SETUP_CADDY="no"
  fi

  if [[ "$SETUP_CADDY" == "yes" ]]; then
    setup_caddy_config
  fi
}

setup_caddy_config() {
  echo
  echo "Caddy requires a domain name for HTTPS certificates."
  echo "The domain must point to this server's IP address."
  echo
  echo "Examples:"
  echo "  - oqto.example.com"
  echo "  - agents.mycompany.io"
  echo "  - localhost (for local testing without TLS)"
  echo

  if [[ -n "$OQTO_DOMAIN" ]]; then
    DOMAIN="$OQTO_DOMAIN"
  else
    DOMAIN=$(prompt_input "Domain name" "localhost")
  fi

  # Strip protocol prefix if user included it
  DOMAIN="${DOMAIN#https://}"
  DOMAIN="${DOMAIN#http://}"
  # Strip trailing slash
  DOMAIN="${DOMAIN%/}"

  if [[ "$DOMAIN" == "localhost" ]]; then
    log_warn "Using localhost - HTTPS will not be enabled"
  else
    log_info "Caddy will obtain TLS certificate for: $DOMAIN"
  fi
}

install_caddy() {
  if [[ "$SETUP_CADDY" != "yes" ]]; then
    return 0
  fi

  log_step "Installing Caddy"

  if command_exists caddy; then
    log_success "Caddy already installed: $(caddy version 2>/dev/null | head -1)"
    return 0
  fi

  case "$OS" in
  macos)
    if command_exists brew; then
      log_info "Installing Caddy via Homebrew..."
      brew install caddy
    else
      log_warn "Homebrew not found. Please install Caddy manually:"
      log_info "  brew install caddy"
      log_info "  or download from: https://caddyserver.com/download"
    fi
    ;;
  linux)
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros)
      log_info "Installing Caddy via pacman..."
      sudo pacman -S --noconfirm caddy
      ;;
    debian | ubuntu | pop | linuxmint)
      log_info "Installing Caddy via apt..."
      sudo apt-get install -y debian-keyring debian-archive-keyring apt-transport-https curl
      curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
      curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
      apt_update_once force
      sudo apt-get install -y caddy
      ;;
    fedora)
      log_info "Installing Caddy via dnf..."
      sudo dnf install -y 'dnf-command(copr)'
      sudo dnf copr enable -y @caddy/caddy
      sudo dnf install -y caddy
      ;;
    *)
      log_warn "Unknown distribution. Installing Caddy via GitHub release..."
      install_caddy_binary
      ;;
    esac
    ;;
  esac

  if command_exists caddy; then
    log_success "Caddy installed successfully"
  else
    log_warn "Caddy installation may have failed. Please install manually."
  fi
}

install_caddy_binary() {
  local caddy_version="2.9.1"
  local arch="$ARCH"

  case "$arch" in
  x86_64) arch="amd64" ;;
  aarch64) arch="arm64" ;;
  esac

  local caddy_url="https://github.com/caddyserver/caddy/releases/download/v${caddy_version}/caddy_${caddy_version}_linux_${arch}.tar.gz"

  log_info "Downloading Caddy ${caddy_version}..."
  curl -sL "$caddy_url" | sudo tar -xzC /usr/local/bin caddy
  sudo chmod +x /usr/local/bin/caddy
}

generate_caddyfile() {
  if [[ "$SETUP_CADDY" != "yes" ]]; then
    return 0
  fi

  log_step "Generating Caddyfile"

  local caddy_config_dir="/etc/caddy"
  local caddyfile="${caddy_config_dir}/Caddyfile"

  # Determine ports and paths
  local backend_port="8080"
  local frontend_dir="/var/www/oqto"

  # Create config directory
  if [[ ! -d "$caddy_config_dir" ]]; then
    sudo mkdir -p "$caddy_config_dir"
  fi

  # Generate Caddyfile
  #
  # Route structure:
  # - /api/* -> backend (strip /api prefix)
  # - /ws    -> backend WebSocket
  # - /session/* -> backend (terminal, files, code proxies)
  # - /health, /auth/*, /me, /admin/* -> backend
  # - Everything else -> frontend
  #
  if [[ "$DOMAIN" == "localhost" ]]; then
    # Local development - no TLS
    sudo tee "$caddyfile" >/dev/null <<EOF
# Oqto Caddyfile - Local Development
# Generated by setup.sh on $(date)

:80 {
    # Backend API (all routes are under /api on the backend)
    handle /api/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Frontend (static files with SPA fallback)
    handle {
        root * ${frontend_dir}
        try_files {path} /index.html
        file_server
    }
    
    log {
        output file /var/log/caddy/oqto.log
    }
}
EOF
  else
    # Production - with TLS
    sudo tee "$caddyfile" >/dev/null <<EOF
# Oqto Caddyfile - Production
# Generated by setup.sh on $(date)
# Domain: ${DOMAIN}

${DOMAIN} {
    # Backend API (all routes are under /api on the backend)
    handle /api/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Frontend (static files with SPA fallback)
    handle {
        root * ${frontend_dir}
        try_files {path} /index.html
        file_server
    }
    
    # Security headers
    header {
        X-Content-Type-Options nosniff
        X-Frame-Options DENY
        Referrer-Policy strict-origin-when-cross-origin
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        X-XSS-Protection "1; mode=block"
        -Server
    }
    
    log {
        output file /var/log/caddy/oqto.log
        format json
    }
    
    # Enable compression
    encode gzip zstd
}

# Redirect HTTP to HTTPS
http://${DOMAIN} {
    redir https://${DOMAIN}{uri} permanent
}
EOF
  fi

  # Create log directory
  sudo mkdir -p /var/log/caddy

  log_success "Caddyfile generated: $caddyfile"
}

install_caddy_service() {
  if [[ "$SETUP_CADDY" != "yes" ]]; then
    return 0
  fi

  log_step "Installing Caddy service"

  case "$OS" in
  linux)
    # Caddy usually comes with systemd service, but ensure it's enabled
    if [[ -f /lib/systemd/system/caddy.service ]] || [[ -f /etc/systemd/system/caddy.service ]]; then
      log_info "Enabling Caddy service..."
      sudo systemctl daemon-reload
      sudo systemctl enable caddy

      if confirm "Start Caddy now?"; then
        # Use restart (not start) to pick up the new Caddyfile
        # start is a no-op if caddy is already running with the default config
        sudo systemctl restart caddy
        log_success "Caddy service started with Oqto config"
        log_info "Check status with: sudo systemctl status caddy"
      fi
    else
      log_warn "Caddy systemd service not found. You may need to configure it manually."
    fi
    ;;
  macos)
    log_info "On macOS, Caddy can be started with:"
    log_info "  sudo caddy start --config /etc/caddy/Caddyfile"
    log_info "Or use Homebrew services:"
    log_info "  brew services start caddy"
    ;;
  esac
}

