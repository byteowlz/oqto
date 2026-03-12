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
: "${SEARXNG_WATCHDOG_INTERVAL:=45s}"
: "${SEARXNG_WATCHDOG_QUERY:=healthcheck}"

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

  # Migrate known legacy path if canonical path is missing.
  if [[ "$service_type" == "system" ]]; then
    migrate_legacy_searxng_path "$searxng_base" "$searxng_user"
  fi

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

  # 7. Install watchdog timer to auto-recover degraded runtime states
  install_searxng_watchdog "$service_type"

  # 8. Configure sx to use local instance
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

migrate_legacy_searxng_path() {
  local canonical_base="$1"
  local user="$2"

  if [[ -x "${canonical_base}/venv/bin/python" ]]; then
    return
  fi

  local candidates=(
    "/home/octo/.local/share/searxng"
    "/home/tommy/.local/share/searxng"
  )

  for legacy in "${candidates[@]}"; do
    if [[ "$legacy" == "$canonical_base" ]]; then
      continue
    fi
    if [[ -x "${legacy}/venv/bin/python" ]]; then
      log_warn "Found legacy SearXNG runtime at ${legacy}; migrating to ${canonical_base}"
      sudo mkdir -p "$(dirname "$canonical_base")"
      sudo rm -rf "$canonical_base"
      sudo cp -a "$legacy" "$canonical_base"
      sudo chown -R "$user:$user" "$canonical_base"
      return
    fi
  done
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

stop_conflicting_searxng_on_port() {
  local target_user="$1"

  # Stop any existing searx.webapp listener on the configured local port
  # that is owned by a different user (common after account/user renames).
  local pids
  pids=$(sudo ss -tlnp "sport = :${SEARXNG_PORT}" 2>/dev/null | awk -F 'pid=' '/pid=/ {split($2,a,",|\)"); print a[1]}' | sort -u)

  if [[ -z "$pids" ]]; then
    return
  fi

  while IFS= read -r pid; do
    [[ -n "$pid" ]] || continue
    local owner cmd
    owner=$(ps -o user= -p "$pid" 2>/dev/null | awk '{print $1}')
    cmd=$(ps -o args= -p "$pid" 2>/dev/null || true)
    if [[ "$cmd" != *"searx.webapp"* ]]; then
      continue
    fi
    if [[ -n "$owner" && "$owner" != "$target_user" ]]; then
      log_warn "Stopping conflicting searx.webapp process on port ${SEARXNG_PORT} (pid=${pid}, user=${owner})"
      sudo kill "$pid" 2>/dev/null || true
    fi
  done <<< "$pids"
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
    local launcher_path="$HOME/.local/bin/searxng-launch"
    mkdir -p "$service_dir" "$HOME/.local/bin"

    cat >"$launcher_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
BASE="${searxng_base}"
PY="\${BASE}/venv/bin/python"
SRC="\${BASE}/searxng-src"
if [[ ! -x "\${PY}" ]]; then
  echo "[searxng-launch] Missing runtime: \${PY}" >&2
  exit 1
fi

# Use dedicated writable tmp dir for sqlite caches to avoid /tmp edge cases
export TMPDIR="\${TMPDIR:-\${BASE}/tmp}"
mkdir -p "\${TMPDIR}"
rm -f "\${TMPDIR}/sxng_cache_DATA_CACHE.db" \
      "\${TMPDIR}/sxng_cache_DATA_CACHE.db-shm" \
      "\${TMPDIR}/sxng_cache_DATA_CACHE.db-wal" 2>/dev/null || true

cd "\${SRC}"
exec "\${PY}" -m searx.webapp
EOF
    chmod +x "$launcher_path"

    cat >"${service_dir}/searxng.service" <<EOF
[Unit]
Description=SearXNG local search engine
After=default.target
StartLimitIntervalSec=300
StartLimitBurst=5

[Service]
Type=simple
Environment=SEARXNG_SETTINGS_PATH=${searxng_settings}
Environment=SEARXNG_BASE=${searxng_base}
ExecStart=${launcher_path}
Restart=on-failure
RestartSec=10

[Install]
WantedBy=default.target
EOF

    systemctl --user daemon-reload
    systemctl --user reset-failed searxng 2>/dev/null || true
    systemctl --user enable searxng

    if confirm "Start SearXNG now?"; then
      systemctl --user restart searxng
      if curl -fsS "http://${SEARXNG_BIND}:${SEARXNG_PORT}" >/dev/null 2>&1; then
        log_success "SearXNG started (user service)"
      else
        log_warn "SearXNG started but health check failed. Check: systemctl --user status searxng"
      fi
      log_info "Check status: systemctl --user status searxng"
    fi

  else
    # System-level systemd service
    local launcher_path="/usr/local/bin/searxng-launch"

    stop_conflicting_searxng_on_port "$user"

    sudo tee "$launcher_path" >/dev/null <<EOF
#!/usr/bin/env bash
set -euo pipefail
BASE="${searxng_base}"
PY="\${BASE}/venv/bin/python"
SRC="\${BASE}/searxng-src"
if [[ ! -x "\${PY}" ]]; then
  echo "[searxng-launch] Missing runtime: \${PY}" >&2
  exit 1
fi

# Use dedicated writable tmp dir for sqlite caches to avoid /tmp edge cases
export TMPDIR="\${TMPDIR:-\${BASE}/tmp}"
mkdir -p "\${TMPDIR}"
rm -f "\${TMPDIR}/sxng_cache_DATA_CACHE.db" \
      "\${TMPDIR}/sxng_cache_DATA_CACHE.db-shm" \
      "\${TMPDIR}/sxng_cache_DATA_CACHE.db-wal" 2>/dev/null || true

cd "\${SRC}"
exec "\${PY}" -m searx.webapp
EOF
    sudo chmod +x "$launcher_path"

    sudo tee /etc/systemd/system/searxng.service >/dev/null <<EOF
[Unit]
Description=SearXNG local search engine
After=network.target valkey.service redis.service
StartLimitIntervalSec=300
StartLimitBurst=5

[Service]
Type=simple
User=${user}
Group=${user}
Environment=SEARXNG_SETTINGS_PATH=${searxng_settings}
Environment=SEARXNG_BASE=${searxng_base}
ExecStart=${launcher_path}
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

    sudo systemctl daemon-reload
    sudo systemctl reset-failed searxng 2>/dev/null || true
    sudo systemctl enable searxng

    if confirm "Start SearXNG now?"; then
      sudo systemctl restart searxng
      if curl -fsS "http://${SEARXNG_BIND}:${SEARXNG_PORT}" >/dev/null 2>&1; then
        log_success "SearXNG started (system service)"
      else
        log_warn "SearXNG started but health check failed. Check: sudo systemctl status searxng"
      fi
      log_info "Check status: sudo systemctl status searxng"
    fi
  fi
}

install_searxng_watchdog() {
  local service_type="$1"

  local probe_script_content
  probe_script_content='#!/usr/bin/env bash
set -euo pipefail

SEARX_URL="http://'"${SEARXNG_BIND}:${SEARXNG_PORT}"'"
QUERY="'"${SEARXNG_WATCHDOG_QUERY}"'"

if ! response=$(curl -fsS --max-time 12 "${SEARX_URL}/search?q=${QUERY}&format=json"); then
  echo "[searxng-watchdog] search API unreachable"
  exit 1
fi

if ! jq -e ".number_of_results != null" >/dev/null 2>&1 <<<"${response}"; then
  echo "[searxng-watchdog] invalid JSON response"
  exit 1
fi

if jq -e ".unresponsive_engines[]? | .[1] | strings | test(\"unexpected crash|readonly database|OperationalError\"; \"i\")" >/dev/null 2>&1 <<<"${response}"; then
  echo "[searxng-watchdog] detected crashed engines in response"
  exit 1
fi

# Catch silent cache corruption: readonly sqlite errors in fresh logs
journal_scope=()
if systemctl --user status searxng >/dev/null 2>&1; then
  journal_scope+=(--user)
fi
if journalctl "${journal_scope[@]}" -u searxng --since "2 minutes ago" --no-pager 2>/dev/null | rg -qi "readonly database|sqlite3\.OperationalError"; then
  echo "[searxng-watchdog] detected sqlite cache write errors"
  exit 1
fi
'

  if [[ "$service_type" == "user" ]]; then
    local service_dir="$HOME/.config/systemd/user"
    local local_bin_dir="$HOME/.local/bin"
    local probe_script="$local_bin_dir/searxng-watchdog-check"
    local probe_runner="$local_bin_dir/searxng-healthcheck-run"
    mkdir -p "$service_dir" "$local_bin_dir"

    cat >"$probe_script" <<EOF
${probe_script_content}
EOF
    chmod +x "$probe_script"

    cat >"$probe_runner" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if ! "${probe_script}"; then
  echo "[searxng-healthcheck] probe failed, restarting searxng"
  systemctl --user restart searxng
fi
EOF
    chmod +x "$probe_runner"

    cat >"${service_dir}/searxng-healthcheck.service" <<EOF
[Unit]
Description=SearXNG health watchdog
After=searxng.service

[Service]
Type=oneshot
ExecStart=${probe_runner}
EOF

    cat >"${service_dir}/searxng-healthcheck.timer" <<EOF
[Unit]
Description=Run SearXNG health watchdog every ${SEARXNG_WATCHDOG_INTERVAL}

[Timer]
OnBootSec=30s
OnUnitActiveSec=${SEARXNG_WATCHDOG_INTERVAL}
Unit=searxng-healthcheck.service

[Install]
WantedBy=timers.target
EOF

    systemctl --user daemon-reload
    systemctl --user enable searxng-healthcheck.timer
    if systemctl --user is-active searxng >/dev/null 2>&1; then
      systemctl --user start searxng-healthcheck.timer
    fi

    log_success "SearXNG watchdog installed (user): searxng-healthcheck.timer"
  else
    local probe_script="/usr/local/bin/searxng-watchdog-check"
    local probe_runner="/usr/local/bin/searxng-healthcheck-run"

    sudo tee "$probe_script" >/dev/null <<EOF
${probe_script_content}
EOF
    sudo chmod +x "$probe_script"

    sudo tee "$probe_runner" >/dev/null <<EOF
#!/usr/bin/env bash
set -euo pipefail
if ! "${probe_script}"; then
  echo "[searxng-healthcheck] probe failed, restarting searxng"
  systemctl restart searxng
fi
EOF
    sudo chmod +x "$probe_runner"

    sudo tee /etc/systemd/system/searxng-healthcheck.service >/dev/null <<EOF
[Unit]
Description=SearXNG health watchdog
After=searxng.service

[Service]
Type=oneshot
ExecStart=${probe_runner}
EOF

    sudo tee /etc/systemd/system/searxng-healthcheck.timer >/dev/null <<EOF
[Unit]
Description=Run SearXNG health watchdog every ${SEARXNG_WATCHDOG_INTERVAL}

[Timer]
OnBootSec=30s
OnUnitActiveSec=${SEARXNG_WATCHDOG_INTERVAL}
Unit=searxng-healthcheck.service

[Install]
WantedBy=timers.target
EOF

    sudo systemctl daemon-reload
    sudo systemctl enable searxng-healthcheck.timer
    if sudo systemctl is-active searxng >/dev/null 2>&1; then
      sudo systemctl start searxng-healthcheck.timer
    fi

    log_success "SearXNG watchdog installed (system): searxng-healthcheck.timer"
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

