# ==============================================================================
# Configuration Generation
# ==============================================================================

generate_jwt_secret() {
  openssl rand -base64 48
}

generate_password_hash() {
  local password="$1"
  # Use oqtoctl (same bcrypt implementation as the backend) for guaranteed compatibility
  # Try PATH first, then known install locations (PATH cache may be stale)
  local oqtoctl_bin=""
  if command_exists oqtoctl; then
    oqtoctl_bin="oqtoctl"
  elif [[ -x "${TOOLS_INSTALL_DIR}/oqtoctl" ]]; then
    oqtoctl_bin="${TOOLS_INSTALL_DIR}/oqtoctl"
  fi
  if [[ -n "$oqtoctl_bin" ]] && "$oqtoctl_bin" hash-password --help >/dev/null 2>&1; then
    echo -n "$password" | "$oqtoctl_bin" hash-password
  elif command_exists python3 && python3 -c "import bcrypt" 2>/dev/null; then
    # Fallback: python3 with bcrypt module
    python3 -c "import bcrypt, base64; pwd = base64.b64decode('$([[ -n "$password" ]] && echo -n "$password" | base64 -w0 || echo)').decode(); print(bcrypt.hashpw(pwd.encode(), bcrypt.gensalt(12)).decode())"
  else
    log_error "Cannot generate password hash. Install oqtoctl or python3 with bcrypt."
    exit 1
  fi
}

write_skdlr_agent_config() {
  local skdlr_config="/etc/oqto/skdlr-agent.toml"
  local sandbox_config="/etc/oqto/sandbox.toml"

  log_info "Writing skdlr agent config to $skdlr_config"

  sudo mkdir -p /etc/oqto

  # Ensure sandbox config exists for oqto-sandbox
  if [[ ! -f "$sandbox_config" ]]; then
    log_info "Creating default sandbox config at $sandbox_config"
    sudo cp "$SCRIPT_DIR/backend/crates/oqto/examples/sandbox.toml" "$sandbox_config"
    sudo chmod 644 "$sandbox_config"
  fi

  sudo tee "$skdlr_config" >/dev/null <<'EOF'
# skdlr config for Oqto sandboxed agents
# Forces all scheduled commands through oqto-sandbox

[executor]
wrapper = "oqto-sandbox"
wrapper_args = ["--config", "/etc/oqto/sandbox.toml", "--workspace", "{workdir}", "--"]
EOF

  sudo chmod 644 "$skdlr_config"
}

generate_config() {
  log_step "Generating configuration"

  # Create config directories
  mkdir -p "$OQTO_CONFIG_DIR"
  mkdir -p "$OQTO_DATA_DIR"

  local config_file="$OQTO_CONFIG_DIR/config.toml"

  if [[ -f "$config_file" ]]; then
    if confirm "Config file exists at $config_file. Overwrite?"; then
      cp "$config_file" "${config_file}.backup.$(date +%Y%m%d%H%M%S)"
      log_info "Backed up existing config"
    else
      log_info "Keeping existing config"
      return 0
    fi
  fi

  # Gather configuration values
  log_info "Configuring Oqto..."

  # Workspace directory (use saved value as default if available)
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    WORKSPACE_DIR=$(prompt_input "Workspace directory" "${WORKSPACE_DIR:-$HOME/oqto/workspace}")
  else
    local default_workspace="/home/{linux_username}/oqto"
    WORKSPACE_DIR=$(prompt_input "Workspace base directory (user dirs created here)" "${WORKSPACE_DIR:-$default_workspace}")
  fi

  # Auth configuration (use globals so state persistence works)
  local dev_user_hash admin_user_hash=""

  if [[ "$OQTO_DEV_MODE" == "true" ]]; then
    log_info "Setting up development user..."
    dev_user_id=$(prompt_input "Dev user ID" "${dev_user_id:-dev}")
    dev_user_name=$(prompt_input "Dev user name" "${dev_user_name:-Developer}")
    dev_user_email=$(prompt_input "Dev user email" "${dev_user_email:-dev@localhost}")
    local dev_password
    dev_password=$(prompt_password "Dev user password")

    if [[ -n "$dev_password" ]]; then
      log_info "Generating password hash..."
      local oqtoctl_bin=""
      command_exists oqtoctl && oqtoctl_bin="oqtoctl"
      [[ -z "$oqtoctl_bin" && -x "${TOOLS_INSTALL_DIR}/oqtoctl" ]] && oqtoctl_bin="${TOOLS_INSTALL_DIR}/oqtoctl"
      if [[ -n "$oqtoctl_bin" ]]; then
        dev_user_hash=$("$oqtoctl_bin" hash-password --password "$dev_password")
      else
        dev_user_hash=$(generate_password_hash "$dev_password")
      fi
      dev_password=""
    else
      dev_user_hash=""
    fi
  elif [[ "$PRODUCTION_MODE" == "true" ]]; then
    # Production mode - admin hash is generated in create_admin_user_db step
    admin_user_hash=""
  fi

  # Use JWT secret from production setup or generate new one
  local jwt_secret
  if [[ -n "$JWT_SECRET" ]]; then
    jwt_secret="$JWT_SECRET"
  else
    jwt_secret=$(generate_jwt_secret)
  fi

  # EAVS configuration
  # EAVS is always used - it's the mandatory LLM proxy layer.
  # Provider API keys are configured via EAVS, not directly.
  local eavs_enabled="true"
  local eavs_base_url="http://127.0.0.1:${EAVS_PORT}"
  local eavs_container_url="http://host.docker.internal:${EAVS_PORT}"
  EAVS_ENABLED="true"

  if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
    eavs_container_url=$(prompt_input "EAVS container URL (for Docker access)" "$eavs_container_url")
  fi

  # Linux user isolation (multi-user local mode only)
  local linux_users_enabled="false"
  if [[ "$SELECTED_USER_MODE" == "multi" && "$SELECTED_BACKEND_MODE" == "local" && "$OS" == "linux" ]]; then
    echo
    echo "Linux user isolation provides security by running each user's"
    echo "agent processes as a separate Linux user account."
    echo
    echo "This requires:"
    echo "  - sudo privileges (for creating users and sudoers rules)"
    echo "  - The 'oqto' group will be created"
    echo "  - Sudoers rules will allow managing octo_* users"
    echo
    if confirm "Enable Linux user isolation? (requires sudo)"; then
      linux_users_enabled="true"
      LINUX_USERS_ENABLED="true"
    fi
  fi

  # Determine Pi runtime mode based on backend mode and user mode
  # (needed early for runner_socket_pattern in [local] section)
  local pi_runtime_mode="local"
  if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
    pi_runtime_mode="container"
  elif [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
    pi_runtime_mode="runner"
  fi

  # Write config file
  log_info "Writing config to $config_file"

  cat >"$config_file" <<EOF
# Oqto Configuration
# Generated by setup.sh on $(date)

"\$schema" = "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/oqto/oqto.backend.config.schema.json"

profile = "default"

[logging]
level = "$OQTO_LOG_LEVEL"

[runtime]
timeout = 60
fail_fast = true

[backend]
mode = "$SELECTED_BACKEND_MODE"

[container]
runtime = "${CONTAINER_RUNTIME:-docker}"
default_image = "oqto-dev:latest"
base_port = 41820
EOF

  if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
    local runner_socket_line=""
    if [[ "$pi_runtime_mode" == "runner" ]]; then
      runner_socket_line='runner_socket_pattern = "/run/oqto/runner-sockets/{user}/oqto-runner.sock"'
    fi

    cat >>"$config_file" <<EOF

[local]
enabled = true
fileserver_binary = "oqto-files"
ttyd_binary = "ttyd"
workspace_dir = "$WORKSPACE_DIR"
single_user = $([[ "$SELECTED_USER_MODE" == "single" ]] && echo "true" || echo "false")
cleanup_on_startup = true
stop_sessions_on_shutdown = true
${runner_socket_line}

[local.linux_users]
enabled = $linux_users_enabled
prefix = "oqto_"
uid_start = 2000
group = "oqto"
shell = "/bin/zsh"
use_sudo = true
create_home = true
EOF
  fi

  cat >>"$config_file" <<EOF

[eavs]
enabled = true
base_url = "$eavs_base_url"
master_key = "$EAVS_MASTER_KEY"
EOF

  if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
    echo "container_url = \"$eavs_container_url\"" >>"$config_file"
  fi

  # Determine allowed origins for CORS
  local allowed_origins=""
  if [[ "$PRODUCTION_MODE" == "true" && -n "$DOMAIN" && "$DOMAIN" != "localhost" ]]; then
    allowed_origins="allowed_origins = [\"https://${DOMAIN}\"]"
  fi

  cat >>"$config_file" <<EOF

[auth]
dev_mode = $OQTO_DEV_MODE
EOF

  # Add JWT secret for production mode (uncommented)
  if [[ "$PRODUCTION_MODE" == "true" ]]; then
    cat >>"$config_file" <<EOF
jwt_secret = "$jwt_secret"
EOF
  else
    cat >>"$config_file" <<EOF
# jwt_secret = "$jwt_secret"
EOF
  fi

  # Add CORS origins if configured
  if [[ -n "$allowed_origins" ]]; then
    echo "$allowed_origins" >>"$config_file"
  fi

  if [[ "$OQTO_DEV_MODE" == "true" && -n "${dev_user_hash:-}" ]]; then
    cat >>"$config_file" <<EOF

[[auth.dev_users]]
id = "$dev_user_id"
name = "$dev_user_name"
email = "$dev_user_email"
password_hash = "$dev_user_hash"
role = "admin"
EOF
  fi

  # Pi (Main Chat) configuration
  # Pi default provider/model (agents can switch at runtime)
  local default_provider="anthropic"
  local default_model="claude-sonnet-4-20250514"

  cat >>"$config_file" <<EOF

[pi]
enabled = true
executable = "pi"
default_provider = "$default_provider"
default_model = "$default_model"
runtime_mode = "$pi_runtime_mode"
EOF

  cat >>"$config_file" <<EOF

[onboarding_templates]
repo_url = "${ONBOARDING_TEMPLATES_REPO:-$ONBOARDING_TEMPLATES_REPO_DEFAULT}"
cache_path = "${ONBOARDING_TEMPLATES_PATH:-$ONBOARDING_TEMPLATES_PATH_DEFAULT}"
sync_enabled = true
sync_interval_seconds = 300
use_embedded_fallback = true
branch = "main"
subdirectory = "agents/main"
EOF

  cat >>"$config_file" <<EOF

[templates]
repo_path = "${PROJECT_TEMPLATES_PATH:-$PROJECT_TEMPLATES_PATH_DEFAULT}"
type = "remote"
sync_on_list = true
sync_interval_seconds = 120
EOF

  cat >>"$config_file" <<EOF

[feedback]
public_dropbox = "${FEEDBACK_PUBLIC_DROPBOX:-/usr/local/share/oqto/issues}"
private_archive = "${FEEDBACK_PRIVATE_ARCHIVE:-/var/lib/oqto/issue-archive}"
keep_public = true
sync_interval_seconds = 60
EOF

  # runner_socket_pattern is added inline in the [local] block above

  # Runner config (used by oqto-runner daemon)
  if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
    cat >>"$config_file" <<EOF

[runner]
pi_sessions_dir = "~/.pi/agent/sessions"
memories_dir = "~/.local/share/mmry"
EOF
  fi

  # hstry (chat history) config
  cat >>"$config_file" <<EOF

[hstry]
enabled = true
binary = "hstry"
EOF

  cat >>"$config_file" <<EOF

[sessions]
auto_attach = "off"
auto_attach_scan = true
autoresume = true
max_concurrent_sessions = 20
idle_timeout_minutes = 30
idle_check_interval_seconds = 300

[scaffold]
binary = "byt"
subcommand = "new"
template_arg = "--template"
output_arg = "--output"
github_arg = "--github"
private_arg = "--private"
description_arg = "--description"

[mmry]
enabled = true
user_base_port = 48000
user_port_range = 1000

[voice]
enabled = ${VOICE_ENABLED:-false}
stt_url = "${VOICE_STT_URL:-ws://127.0.0.1:8765}"
tts_url = "${VOICE_TTS_URL:-ws://127.0.0.1:8766}"
vad_timeout_ms = 1500
default_voice = "af_heart"
default_speed = 1.0
auto_language_detect = true
tts_muted = false
continuous_mode = true
default_visualizer = "orb"
interrupt_word_count = 2
interrupt_backoff_ms = 5000

[voice.visualizer_voices.orb]
voice = "af_heart"
speed = 1.0

[voice.visualizer_voices.kitt]
voice = "am_adam"
speed = 1.1

[agent_browser]
enabled = true
binary = "${BROWSERD_DEPLOY_DIR:-$HOME/.local/lib/oqto-browserd}/bin/oqto-browserd.js"
headed = false
stream_port_base = 30000
stream_port_range = 10000
EOF

  log_success "Configuration written to $config_file"

  # API keys are now managed by EAVS, not stored in oqto's env file

  # Create workspace directory
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    mkdir -p "$WORKSPACE_DIR"
    log_success "Workspace directory created: $WORKSPACE_DIR"

    # Copy AGENTS.md template if not exists
    if [[ ! -f "$WORKSPACE_DIR/AGENTS.md" ]]; then
      log_info "Created default AGENTS.md in workspace"
    fi
  fi

  # Write skdlr agent wrapper config for sandboxed schedules
  if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
    write_skdlr_agent_config
  fi

  # Save admin credentials for post-setup user creation
  if [[ "$PRODUCTION_MODE" == "true" && -n "$ADMIN_USERNAME" ]]; then
    local creds_file="$OQTO_CONFIG_DIR/.admin_setup"
    # Write username/email normally, but use printf for the bcrypt hash
    # to avoid $2b$ being expanded as positional params when sourced
    {
      echo "ADMIN_USERNAME=\"$ADMIN_USERNAME\""
      echo "ADMIN_EMAIL=\"$ADMIN_EMAIL\""
      printf "ADMIN_PASSWORD_HASH='%s'\n" "$admin_user_hash"
    } >"$creds_file"
    chmod 600 "$creds_file"
    log_info "Admin credentials saved for database setup"
  fi
}

