# ==============================================================================
# SearXNG Installation (local search engine for agents)
# ==============================================================================
#
# SearXNG is a privacy-respecting metasearch engine. We install it locally so
# agents can use `sx` for web searches without depending on external APIs.
#
# Architecture:
#   - Runs under the backend user (current user in single-user, 'oqto' in multi-user)
#   - Binds to 127.0.0.1:8888 (local only, no external exposure)
#   - JSON API enabled so `sx` can query it programmatically
#   - Valkey (Redis-compatible) for rate limiting and caching
#   - Managed via systemd user service (single-user) or system service (multi-user)
#
# ==============================================================================

: "${SEARXNG_PORT:=8888}"
: "${SEARXNG_BIND:=127.0.0.1}"

install_searxng() {
  log_step "Installing SearXNG (local search engine)"

  # Determine install paths based on user mode
  local searxng_base searxng_user service_type
  if [[ "$SELECTED_USER_MODE" == "multi" ]]; then
    searxng_user="oqto"
    searxng_base="${OQTO_HOME}/.local/share/searxng"
    service_type="system"
  else
    searxng_user="$(whoami)"
    searxng_base="${XDG_DATA_HOME}/searxng"
    service_type="user"
  fi

  local searxng_src="${searxng_base}/searxng-src"
  local searxng_venv="${searxng_base}/venv"
  local searxng_settings="${searxng_base}/settings.yml"

  # 1. Install system dependencies
  install_searxng_deps

  # 2. Install Valkey (Redis-compatible key-value store)
  install_valkey

  # 3. Clone SearXNG source
  log_info "Setting up SearXNG in ${searxng_base}..."
  if [[ "$service_type" == "system" ]]; then
    sudo mkdir -p "$searxng_base"
    sudo chown "$searxng_user:$searxng_user" "$searxng_base"
  else
    mkdir -p "$searxng_base"
  fi

  if [[ -d "$searxng_src/.git" ]]; then
    log_info "Updating SearXNG source..."
    run_as_searxng_user "$service_type" "$searxng_user" \
      "git -C '$searxng_src' pull --ff-only" || true
  else
    log_info "Cloning SearXNG..."
    run_as_searxng_user "$service_type" "$searxng_user" \
      "git clone --depth 1 'https://github.com/searxng/searxng' '$searxng_src'"
  fi

  # 4. Create virtualenv and install dependencies
  if [[ ! -d "$searxng_venv" ]]; then
    log_info "Creating Python virtualenv..."
    run_as_searxng_user "$service_type" "$searxng_user" \
      "python3 -m venv '$searxng_venv'"
  fi

  log_info "Installing SearXNG Python dependencies..."
  run_as_searxng_user "$service_type" "$searxng_user" \
    "'$searxng_venv/bin/pip' install -U pip setuptools wheel"
  run_as_searxng_user "$service_type" "$searxng_user" \
    "'$searxng_venv/bin/pip' install -r '$searxng_src/requirements.txt'"
  run_as_searxng_user "$service_type" "$searxng_user" \
    "cd '$searxng_src' && '$searxng_venv/bin/pip' install --use-pep517 --no-build-isolation -e ."

  # 5. Generate settings.yml with JSON API enabled
  generate_searxng_settings "$searxng_settings" "$service_type" "$searxng_user"

  # 6. Install systemd service
  install_searxng_service "$searxng_base" "$searxng_venv" "$searxng_settings" "$service_type" "$searxng_user"

  # 7. Configure sx to use local instance
  configure_sx_for_searxng

  log_success "SearXNG installed and configured"
  log_info "SearXNG API: http://${SEARXNG_BIND}:${SEARXNG_PORT}"
  log_info "Test with: sx 'hello world'"
}

# Helper: run a command as the SearXNG user
run_as_searxng_user() {
  local service_type="$1"
  local user="$2"
  local cmd="$3"

  if [[ "$service_type" == "system" ]]; then
    sudo -H -u "$user" bash -c "$cmd"
  else
    bash -c "$cmd"
  fi
}

install_searxng_deps() {
  log_info "Installing SearXNG system dependencies..."

  case "$OS" in
  linux)
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros)
      sudo pacman -S --noconfirm --needed \
        python python-pip python-virtualenv \
        git base-devel libxslt zlib libffi openssl
      ;;
    debian | ubuntu | pop | linuxmint)
      apt_update_once
      sudo apt-get install -y \
        python3-dev python3-babel python3-venv python-is-python3 \
        git build-essential libxslt-dev zlib1g-dev libffi-dev libssl-dev
      ;;
    fedora | centos | rhel | rocky | alma)
      sudo dnf install -y \
        python3-devel python3-babel python3-virtualenv \
        git gcc libxslt-devel zlib-devel libffi-devel openssl-devel
      ;;
    opensuse* | suse*)
      sudo zypper install -y \
        python3-devel python3-Babel python3-virtualenv \
        git gcc libxslt-devel zlib-devel libffi-devel libopenssl-devel
      ;;
    *)
      log_warn "Unknown distribution. Please install Python 3 dev packages manually."
      ;;
    esac
    ;;
  macos)
    if command_exists brew; then
      brew install python3 libxslt
    else
      log_warn "Homebrew not found. Please install Python 3 manually."
    fi
    ;;
  esac
}

install_valkey() {
  if command_exists valkey-server; then
    log_success "Valkey already installed: $(valkey-server --version 2>/dev/null | head -1)"
  elif command_exists redis-server; then
    log_success "Redis already installed (compatible with Valkey): $(redis-server --version 2>/dev/null | head -1)"
  else
    log_info "Installing Valkey (Redis-compatible key-value store)..."

    case "$OS" in
    linux)
      case "$OS_DISTRO" in
      arch | manjaro | endeavouros)
        sudo pacman -S --noconfirm valkey
        ;;
      debian | ubuntu | pop | linuxmint)
        apt_update_once
        # Valkey may not be in default repos, fall back to redis
        if sudo apt-get install -y valkey 2>/dev/null; then
          true
        else
          log_info "Valkey not in repos, installing Redis instead..."
          sudo apt-get install -y redis-server
        fi
        ;;
      fedora | centos | rhel | rocky | alma)
        sudo dnf install -y valkey 2>/dev/null || sudo dnf install -y redis
        ;;
      opensuse* | suse*)
        sudo zypper install -y valkey 2>/dev/null || sudo zypper install -y redis
        ;;
      *)
        log_warn "Please install Valkey or Redis manually."
        ;;
      esac
      ;;
    macos)
      if command_exists brew; then
        brew install valkey 2>/dev/null || brew install redis
      fi
      ;;
    esac
  fi

  # Enable and start Valkey/Redis
  if command_exists valkey-server; then
    if [[ "$OS" == "linux" ]]; then
      sudo systemctl enable --now valkey 2>/dev/null || sudo systemctl enable --now valkey-server 2>/dev/null || true
    fi
    log_success "Valkey is running"
  elif command_exists redis-server; then
    if [[ "$OS" == "linux" ]]; then
      sudo systemctl enable --now redis 2>/dev/null || sudo systemctl enable --now redis-server 2>/dev/null || true
    fi
    log_success "Redis is running"
  fi
}

generate_searxng_settings() {
  local settings_file="$1"
  local service_type="$2"
  local user="$3"

  log_info "Generating SearXNG settings with JSON API enabled..."

  # Generate a random secret key
  local secret_key
  secret_key=$(generate_secure_secret 32)

  # Detect Valkey/Redis URL
  local kv_url="false"
  if command_exists valkey-server; then
    kv_url="valkey://localhost:6379/0"
  elif command_exists redis-server; then
    kv_url="redis://localhost:6379/0"
  fi

  local settings_content
  read -r -d '' settings_content <<EOSETTINGS || true
# SearXNG settings - generated by Oqto setup.sh
# Local instance for agent web search via sx CLI

use_default_settings: true

general:
  debug: false
  instance_name: "Oqto SearXNG"

search:
  safe_search: 0
  autocomplete: "duckduckgo"
  # Enable JSON format for sx API access
  formats:
    - html
    - json

server:
  port: ${SEARXNG_PORT}
  bind_address: "${SEARXNG_BIND}"
  secret_key: "${secret_key}"
  limiter: false
  public_instance: false
  image_proxy: false
  http_protocol_version: "1.0"
  method: "GET"

valkey:
  url: ${kv_url}

ui:
  static_use_hash: true

outgoing:
  request_timeout: 10.0
  max_request_timeout: 15.0
  # Use multiple user agents to avoid blocks
  useragent_suffix: ""

engines:
  - name: duckduckgo
    disabled: false
  - name: google
    disabled: false
  - name: brave
    disabled: false
  - name: wikipedia
    disabled: false
  - name: github
    disabled: false
  - name: stackoverflow
    disabled: false
  - name: arch linux wiki
    disabled: false
  - name: npm
    disabled: false
  - name: crates.io
    disabled: false
  - name: pypi
    disabled: false
EOSETTINGS

  if [[ "$service_type" == "system" ]]; then
    echo "$settings_content" | sudo -u "$user" tee "$settings_file" >/dev/null
  else
    echo "$settings_content" >"$settings_file"
  fi

  log_success "SearXNG settings written to $settings_file"
}

install_searxng_service() {
  local searxng_base="$1"
  local searxng_venv="$2"
  local searxng_settings="$3"
  local service_type="$4"
  local user="$5"

  local searxng_src="${searxng_base}/searxng-src"

  if [[ "$service_type" == "user" ]]; then
    # User-level systemd service
    local service_dir="$HOME/.config/systemd/user"
    mkdir -p "$service_dir"

    cat >"${service_dir}/searxng.service" <<EOF
[Unit]
Description=SearXNG local search engine
After=default.target

[Service]
Type=simple
Environment=SEARXNG_SETTINGS_PATH=${searxng_settings}
WorkingDirectory=${searxng_src}
ExecStart=${searxng_venv}/bin/python -m searx.webapp
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    systemctl --user daemon-reload
    systemctl --user enable searxng

    if confirm "Start SearXNG now?"; then
      systemctl --user start searxng
      log_success "SearXNG started (user service)"
      log_info "Check status: systemctl --user status searxng"
    fi

  else
    # System-level systemd service
    sudo tee /etc/systemd/system/searxng.service >/dev/null <<EOF
[Unit]
Description=SearXNG local search engine
After=network.target valkey.service redis.service

[Service]
Type=simple
User=${user}
Group=${user}
Environment=SEARXNG_SETTINGS_PATH=${searxng_settings}
WorkingDirectory=${searxng_src}
ExecStart=${searxng_venv}/bin/python -m searx.webapp
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

    sudo systemctl daemon-reload
    sudo systemctl enable searxng

    if confirm "Start SearXNG now?"; then
      sudo systemctl start searxng
      log_success "SearXNG started (system service)"
      log_info "Check status: sudo systemctl status searxng"
    fi
  fi
}

configure_sx_for_searxng() {
  local sx_config_dir="${XDG_CONFIG_HOME}/sx"
  local sx_config_file="${sx_config_dir}/config.toml"

  mkdir -p "$sx_config_dir"

  if [[ -f "$sx_config_file" ]]; then
    # Update existing config - just change the URL
    if grep -q "searxng_url" "$sx_config_file"; then
      sed -i "s|searxng_url = .*|searxng_url = \"http://${SEARXNG_BIND}:${SEARXNG_PORT}\"|" "$sx_config_file"
      log_success "Updated sx config with local SearXNG URL"
    else
      echo "searxng_url = \"http://${SEARXNG_BIND}:${SEARXNG_PORT}\"" >>"$sx_config_file"
      log_success "Added SearXNG URL to sx config"
    fi
  else
    cat >"$sx_config_file" <<EOF
"\$schema" = "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/sx/sx.config.schema.json"

# sx configuration - generated by Oqto setup.sh
engine = "searxng"
searxng_url = "http://${SEARXNG_BIND}:${SEARXNG_PORT}"
result_count = 10
safe_search = "none"
http_method = "GET"
timeout = 30.0
history_enabled = true
max_history = 100
EOF
    log_success "Created sx config at $sx_config_file"
  fi
}

