# ==============================================================================
# EAVS Installation (LLM proxy for agents)
# ==============================================================================
#
# EAVS is a bidirectional LLM proxy that:
#   - Routes requests to multiple providers (Anthropic, OpenAI, Google, etc.)
#   - Manages virtual API keys per session with budgets and rate limits
#   - Provides a single endpoint for all LLM access
#   - Oqto creates per-session virtual keys automatically
#
# ==============================================================================

: "${EAVS_PORT:=3033}"
EAVS_MASTER_KEY=""

install_eavs() {
  log_step "Installing EAVS (LLM proxy)"

  # EAVS uses the system keychain for secret storage (keychain: syntax in config).
  # On headless Linux servers, gnome-keyring provides the org.freedesktop.secrets
  # D-Bus service that libsecret needs. Without it, `eavs secret set` fails with:
  #   "The name org.freedesktop.secrets was not provided by any .service files"
  if [[ "$OS" == "linux" ]]; then
    install_eavs_keyring_deps
  fi

  download_or_build_tool eavs

  if ! command_exists eavs; then
    log_error "EAVS installation failed"
    return 1
  fi

  # Install TypeScript adapters for model export (Pi, OpenCode, etc.)
  install_eavs_adapters

  log_success "EAVS installed: $(eavs --version 2>/dev/null | head -1)"
}

# Install eavs TypeScript adapters for 'eavs models export'.
# These live next to the binary so eavs can discover them automatically.
install_eavs_adapters() {
  local eavs_bin
  eavs_bin=$(command -v eavs 2>/dev/null) || return 0
  local eavs_dir
  eavs_dir=$(dirname "$eavs_bin")
  local adapters_dest="${eavs_dir}/adapters"

  # If adapters already exist next to binary (from release tarball), done
  if [[ -d "$adapters_dest" && -f "$adapters_dest/pi/adapter.ts" ]]; then
    log_info "EAVS adapters already installed"
    return 0
  fi

  # Fetch adapters from the eavs repo
  local version
  version=$(get_dep_version eavs)
  local tag="v${version:-main}"

  log_info "Installing EAVS adapters..."
  local tmpdir
  tmpdir=$(mktemp -d)

  if curl -fsSL "https://github.com/byteowlz/eavs/archive/refs/tags/${tag}.tar.gz" |
    tar xz -C "$tmpdir" --strip-components=1 "*/adapters" 2>/dev/null; then
    sudo mkdir -p "$adapters_dest"
    sudo cp -r "$tmpdir/adapters/"* "$adapters_dest/"
    log_success "EAVS adapters installed to $adapters_dest"
  else
    # Try main branch as fallback
    if curl -fsSL "https://github.com/byteowlz/eavs/archive/refs/heads/main.tar.gz" |
      tar xz -C "$tmpdir" --strip-components=1 "*/adapters" 2>/dev/null; then
      sudo mkdir -p "$adapters_dest"
      sudo cp -r "$tmpdir/adapters/"* "$adapters_dest/"
      log_success "EAVS adapters installed from main branch"
    else
      log_warn "Could not fetch EAVS adapters. 'eavs models export' will not work."
    fi
  fi

  rm -rf "$tmpdir"
}

# Install gnome-keyring and libsecret for headless servers.
# These provide the org.freedesktop.secrets D-Bus service that EAVS needs
# for its keychain backend (storing OAuth tokens and API keys securely).
install_eavs_keyring_deps() {
  # Check if the secrets service is already available
  if dbus-send --session --dest=org.freedesktop.secrets \
    --print-reply /org/freedesktop/secrets \
    org.freedesktop.DBus.Peer.Ping &>/dev/null; then
    log_info "Secret service already available"
    return 0
  fi

  log_info "Installing secret service for EAVS keychain support"

  case "$OS_DISTRO" in
  ubuntu | debian | pop)
    sudo apt-get install -y gnome-keyring libsecret-1-0 >/dev/null 2>&1
    ;;
  fedora | centos | rhel | rocky | alma)
    sudo dnf install -y gnome-keyring libsecret >/dev/null 2>&1
    ;;
  arch | manjaro | endeavouros)
    sudo pacman -S --noconfirm --needed gnome-keyring libsecret >/dev/null 2>&1
    ;;
  opensuse*)
    sudo zypper install -y gnome-keyring libsecret >/dev/null 2>&1
    ;;
  *)
    log_warn "Unknown distro '$OS_DISTRO' - install gnome-keyring manually if EAVS keychain fails"
    return 0
    ;;
  esac

  # Start gnome-keyring for the current session
  if command_exists gnome-keyring-daemon; then
    eval "$(gnome-keyring-daemon --start --components=secrets 2>/dev/null)" || true
    log_success "gnome-keyring started for current session"
  fi

  # Enable the socket for future sessions so it auto-starts on login/reboot
  if [[ -f /usr/lib/systemd/user/gnome-keyring-daemon.socket ]]; then
    systemctl --user enable gnome-keyring-daemon.socket 2>/dev/null || true
    systemctl --user start gnome-keyring-daemon.socket 2>/dev/null || true
    log_success "gnome-keyring-daemon.socket enabled for future sessions"
  fi

  # In multi-user mode, also enable gnome-keyring for the oqto system user.
  # The EAVS service runs as oqto and needs D-Bus + gnome-keyring for the
  # keychain: config syntax and `eavs secret set` commands.
  if [[ "$SELECTED_USER_MODE" == "multi" ]] && id oqto &>/dev/null; then
    enable_keyring_for_octo_user
  fi
}

# Enable gnome-keyring-daemon for the oqto system user so that:
#   1. The EAVS system service (User=oqto) can resolve keychain: secrets at startup
#   2. Admins can run `sudo -u oqto dbus-run-session -- eavs secret set <name>`
# Requires linger so oqto's user-level systemd instance persists without a login.
enable_keyring_for_octo_user() {
  log_info "Enabling gnome-keyring for oqto user..."

  # Enable linger so oqto gets a persistent user-level systemd instance
  sudo loginctl enable-linger oqto 2>/dev/null || true

  # Enable the gnome-keyring socket for the oqto user
  if [[ -f /usr/lib/systemd/user/gnome-keyring-daemon.socket ]]; then
    sudo -u oqto systemctl --user enable gnome-keyring-daemon.socket 2>/dev/null || true
    sudo -u oqto systemctl --user start gnome-keyring-daemon.socket 2>/dev/null || true
    log_success "gnome-keyring-daemon.socket enabled for oqto user"
  fi
}

configure_eavs() {
  log_step "Configuring EAVS"

  # Determine config/data paths based on user mode:
  #   single-user: ~/.config/eavs/ (runs as installing user)
  #   multi-user:  ~oqto/.config/eavs/ (runs as oqto system user, same home)
  local eavs_config_dir eavs_data_dir eavs_env_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_config_dir="${XDG_CONFIG_HOME}/eavs"
    eavs_data_dir="${XDG_DATA_HOME:-$HOME/.local/share}/eavs"
    eavs_env_file="${eavs_config_dir}/env"
    mkdir -p "$eavs_config_dir" "$eavs_data_dir"
  else
    eavs_config_dir="${OQTO_HOME}/.config/eavs"
    eavs_data_dir="${OQTO_HOME}/.local/share/eavs"
    eavs_env_file="${eavs_config_dir}/env"
    sudo mkdir -p "$eavs_config_dir" "$eavs_data_dir"
  fi

  local eavs_config_file="${eavs_config_dir}/config.toml"

  # Clear env file on reconfigure to avoid duplicate keys
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    : >"${eavs_env_file}"
  else
    sudo bash -c ": > '${eavs_env_file}'"
  fi

  # Generate master key for oqto to create per-session virtual keys (reuse saved one)
  if [[ -z "${EAVS_MASTER_KEY:-}" ]]; then
    EAVS_MASTER_KEY=$(generate_secure_secret 32)
  else
    log_info "Using saved EAVS master key"
  fi

  # Write master key to env file
  _eavs_env_append "EAVS_MASTER_KEY=${EAVS_MASTER_KEY}"

  # Seed env file with any API keys already in the shell environment.
  # eavs setup add --batch --env-file will pick these up.
  local known_env_vars=(
    OPENAI_API_KEY ANTHROPIC_API_KEY GEMINI_API_KEY GOOGLE_API_KEY
    MISTRAL_API_KEY GROQ_API_KEY XAI_API_KEY OPENROUTER_API_KEY
    CEREBRAS_API_KEY DEEPSEEK_API_KEY
  )
  for var in "${known_env_vars[@]}"; do
    if [[ -n "${!var:-}" ]]; then
      _eavs_env_append "${var}=${!var}"
    fi
  done

  # Lock down env file permissions
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    chmod 600 "${eavs_env_file}"
  else
    sudo chmod 600 "${eavs_env_file}"
  fi

  # Write base eavs config (server, keys, logging -- no providers).
  # eavs setup add will append provider sections to this file.
  local config_content
  config_content=$(
    cat <<EOF
"\$schema" = "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/eavs/eavs.config.schema.json"

# EAVS Configuration - generated by Oqto setup.sh
# Edit this file to add/change LLM providers.
# Docs: https://github.com/byteowlz/eavs

[server]
host = "127.0.0.1"
port = ${EAVS_PORT}

[logging]
default = "stdout"

[analysis]
enabled = true
broadcast_channel_size = 1024

[state]
enabled = true
ttl_secs = 3600
cleanup_interval_secs = 60
max_conversations = 10000

[keys]
enabled = true
require_key = true
database_path = "${eavs_data_dir}/keys.db"
master_key = "env:EAVS_MASTER_KEY"
allow_self_provisioning = false
default_rpm_limit = 60
default_budget_usd = 50.0
update_pricing_on_startup = true
EOF
  )

  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    echo "$config_content" >"$eavs_config_file"
  else
    echo "$config_content" | sudo tee "$eavs_config_file" >/dev/null
    sudo chown -R oqto:oqto "$eavs_config_dir" "$eavs_data_dir"
  fi

  log_success "EAVS base config written to $eavs_config_file"

  # Build import file from oqto.setup.toml custom providers (if any).
  # These are pre-configured providers that get imported non-interactively
  # before the interactive batch wizard runs.
  local import_file=""
  if [[ ${#CUSTOM_PROVIDERS[@]} -gt 0 ]]; then
    import_file=$(mktemp /tmp/eavs-import-XXXXXX.toml)
    for cp_key in "${CUSTOM_PROVIDERS[@]}"; do
      local cp_name
      cp_name=$(echo "$cp_key" | tr '[:space:]' '-' | tr -cd '[:alnum:]_-')
      [[ -z "$cp_name" ]] && continue

      # Write API key value to env file if it's a literal (not env: ref)
      local cp_api_key="${CP_API_KEY[$cp_key]}"
      local cp_api_key_ref=""
      if [[ -n "$cp_api_key" ]]; then
        if [[ "$cp_api_key" == env:* ]]; then
          local env_name="${cp_api_key#env:}"
          local env_val="${!env_name:-}"
          if [[ -n "$env_val" ]]; then
            _eavs_env_append "${env_name}=${env_val}"
          fi
          cp_api_key_ref="env:${env_name}"
        else
          local safe_name
          safe_name=$(echo "$cp_name" | tr '[:lower:]-' '[:upper:]_')
          local env_name="CUSTOM_${safe_name}_API_KEY"
          _eavs_env_append "${env_name}=${cp_api_key}"
          cp_api_key_ref="env:${env_name}"
        fi
      fi

      # Write provider section to import file
      echo "" >>"$import_file"
      echo "[providers.${cp_name}]" >>"$import_file"
      [[ -n "${CP_TYPE[$cp_key]}" ]] && echo "type = \"${CP_TYPE[$cp_key]}\"" >>"$import_file"
      [[ -n "${CP_BASE_URL[$cp_key]}" ]] && echo "base_url = \"${CP_BASE_URL[$cp_key]}\"" >>"$import_file"
      [[ -n "$cp_api_key_ref" ]] && echo "api_key = \"${cp_api_key_ref}\"" >>"$import_file"
      [[ -n "${CP_DEPLOYMENT[$cp_key]}" ]] && echo "deployment = \"${CP_DEPLOYMENT[$cp_key]}\"" >>"$import_file"
      [[ -n "${CP_API_VERSION[$cp_key]}" ]] && echo "api_version = \"${CP_API_VERSION[$cp_key]}\"" >>"$import_file"
      [[ -n "${CP_AWS_REGION[$cp_key]}" ]] && echo "aws_region = \"${CP_AWS_REGION[$cp_key]}\"" >>"$import_file"
      [[ -n "${CP_GCP_PROJECT[$cp_key]}" ]] && echo "gcp_project = \"${CP_GCP_PROJECT[$cp_key]}\"" >>"$import_file"
      [[ -n "${CP_GCP_LOCATION[$cp_key]}" ]] && echo "gcp_location = \"${CP_GCP_LOCATION[$cp_key]}\"" >>"$import_file"
      [[ -n "${CP_TEST_MODEL[$cp_key]}" ]] && echo "test_model = \"${CP_TEST_MODEL[$cp_key]}\"" >>"$import_file"
    done
  fi

  # Ensure the eavs model catalog is downloaded/up-to-date for model selection
  log_info "Updating model catalog from models.dev..."
  eavs models update >/dev/null 2>&1 || true

  echo
  echo "EAVS provider setup -- all provider configuration is handled by eavs."
  echo "Batch mode will detect API keys from the environment, then offer to"
  echo "add custom providers (Azure AI Foundry, Bedrock, Ollama, etc.)."
  echo

  # Build eavs setup add command with flags
  local eavs_add_args=(setup add --batch --config "$eavs_config_file" --env-file "$eavs_env_file")
  if [[ -n "$import_file" ]]; then
    eavs_add_args+=(--import "$import_file")
  fi

  # Run eavs setup add --batch. It will:
  #   1. Import providers from --import file (oqto.setup.toml custom providers)
  #   2. Load --env-file and scan for known API keys (OPENAI_API_KEY, etc.)
  #   3. Ask to confirm each detected key
  #   4. Offer to add custom providers interactively
  #   5. Set the default provider
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs "${eavs_add_args[@]}" </dev/tty || log_warn "eavs setup add failed (you can add providers later with: eavs setup add --config $eavs_config_file)"
  else
    sudo -u oqto eavs "${eavs_add_args[@]}" </dev/tty || log_warn "eavs setup add failed (you can add providers later)"
    sudo chown -R oqto:oqto "$eavs_config_dir" 2>/dev/null
  fi

  # Clean up temp import file
  [[ -n "$import_file" ]] && rm -f "$import_file"

  echo
  log_info "You can add more providers anytime with: eavs setup add --config $eavs_config_file"

  # Extract configured providers from the resulting config for testing
  # and model selection. Parse [providers.NAME] sections, skip "default".
  CONFIGURED_PROVIDERS=""
  local provider_names
  provider_names=$(grep -oP '^\[providers\.(?!default)\K[^\]]+' "$eavs_config_file" 2>/dev/null) || true
  if [[ -n "$provider_names" ]]; then
    CONFIGURED_PROVIDERS=$(echo "$provider_names" | tr '\n' ' ')
    CONFIGURED_PROVIDERS="${CONFIGURED_PROVIDERS% }"
  fi

  if [[ -z "${CONFIGURED_PROVIDERS// }" ]]; then
    log_warn "No providers configured. EAVS will start but agents cannot use any LLM."
    return
  fi

  log_success "Configured providers: $CONFIGURED_PROVIDERS"

  # Offer model shortlist selection for each configured provider.
  # This adds [[providers.<name>.models]] entries to the config for
  # export adapters (Pi, Codex, etc.) to pick up.
  if [[ "$NONINTERACTIVE" != "true" ]]; then
    for provider_name in $CONFIGURED_PROVIDERS; do
      _SELECT_MODELS_RESULT=""
      select_models_for_provider "$provider_name"
      if [[ -n "$_SELECT_MODELS_RESULT" ]]; then
        # Append model shortlist to the config file
        if [[ "$SELECTED_USER_MODE" == "single" ]]; then
          echo "$_SELECT_MODELS_RESULT" >>"$eavs_config_file"
        else
          echo "$_SELECT_MODELS_RESULT" | sudo tee -a "$eavs_config_file" >/dev/null
        fi
      fi
    done
  fi
}

# Helper: append a line to the eavs env file (handles single/multi user mode)
_eavs_env_append() {
  local line="$1"
  local eavs_env_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_env_file="${XDG_CONFIG_HOME}/eavs/env"
    echo "$line" >>"$eavs_env_file"
  else
    eavs_env_file="${OQTO_HOME}/.config/eavs/env"
    echo "$line" | sudo tee -a "$eavs_env_file" >/dev/null
  fi
}

# ==============================================================================
# Dynamic Model Selection from EAVS Catalog
# ==============================================================================
# Queries the live eavs model catalog (models.dev) and lets users pick models
# interactively via fuzzy multi-select. Falls back gracefully:
#   gum filter (best UX) > fzf (good) > numbered list (basic)
# In non-interactive mode, auto-selects the top N newest models.
#
# The catalog is sorted by release date (newest first) by eavs, so the
# pre-selected defaults are always the latest models.

# How many models to pre-select per provider in non-interactive/default mode
DEFAULT_MODEL_COUNT=5

select_models_for_provider() {
  local provider="$1"

  # Result is returned via _SELECT_MODELS_RESULT (not stdout) so that
  # the function runs in the current shell and TUI tools have /dev/tty.
  _SELECT_MODELS_RESULT=""

  # Check if eavs supports the 'models' subcommand (>= 0.5.4)
  if ! eavs models list "$provider" --json >/dev/null 2>&1; then
    log_warn "eavs model catalog not available (upgrade eavs to >= 0.5.4 for model selection)"
    log_info "Skipping model selection for $provider -- all catalog models will be available"
    return
  fi

  # Query the eavs model catalog for this provider
  local catalog_json
  catalog_json=$(eavs models list "$provider" --json 2>/dev/null) || true

  if [[ -z "$catalog_json" || "$catalog_json" == "[]" || "$catalog_json" == "null" ]]; then
    log_warn "No models found in catalog for $provider. Skipping model selection."
    return
  fi

  # Build tab-separated display data using jq
  # Each line: "model_id\tname\t$in/$out\tctx\treasoning\trelease_date"
  local model_lines
  model_lines=$(echo "$catalog_json" | jq -r '.[] |
    def fmt_ctx: .limit.context // 0 |
      if . >= 1000000 then "\(./1000000 | floor)M"
      elif . >= 1000 then "\(./1000 | floor)K"
      else tostring end;
    [
      .id,
      (.name // .id),
      "$\(.cost.input // 0)/$\(.cost.output // 0)",
      fmt_ctx,
      (if .reasoning then "R" else " " end),
      (.release_date // "")[:10]
    ] | @tsv
  ' 2>/dev/null) || true

  if [[ -z "$model_lines" ]]; then
    log_warn "Failed to parse model catalog for $provider"
    return
  fi

  local total_count
  total_count=$(echo "$model_lines" | wc -l)

  # Build formatted display lines for the picker (tab-separated data -> columns)
  local display_lines
  display_lines=$(echo "$model_lines" | awk -F'\t' '{
    printf "%-45s  %s  %-12s  ctx=%-6s  %s\n", $1, $5, $3, $4, $6
  }')

  # Get the top N model IDs for pre-selection
  local default_ids
  default_ids=$(echo "$model_lines" | head -n "$DEFAULT_MODEL_COUNT" | cut -f1)
  local default_csv
  default_csv=$(echo "$default_ids" | paste -sd',' -)

  echo
  echo "  Select models for $provider ($total_count available, newest first):"
  echo "  [R]=reasoning  Costs per 1M tokens  Defaults: top $DEFAULT_MODEL_COUNT newest"
  echo

  local selected_ids=""

  if [[ "$NONINTERACTIVE" == "true" ]]; then
    # Non-interactive: just use the top N
    selected_ids="$default_ids"
    log_info "Auto-selected top $DEFAULT_MODEL_COUNT models for $provider"
  elif command -v gum >/dev/null 2>&1; then
    # gum filter: fuzzy search + multi-select (best UX)
    # Write display lines to temp file to avoid pipe/subshell TTY issues
    local tmpfile selected_tmpfile
    tmpfile=$(mktemp)
    selected_tmpfile=$(mktemp)
    echo "$display_lines" > "$tmpfile"

    # Use gum with file redirection to avoid subshell TTY issues.
    # Note: Pre-selection is skipped because --selected CSV parsing breaks
    # when display lines contain commas. Models are already sorted newest-first.
    gum filter --no-limit \
      --header="Select models for $provider (tab=toggle, enter=confirm, ctrl+c=cancel)" \
      --placeholder="Type to filter... (top $DEFAULT_MODEL_COUNT are recommended)" \
      --height=20 < "$tmpfile" > "$selected_tmpfile" 2>/dev/null || true

    selected_ids=$(awk '{print $1}' "$selected_tmpfile" 2>/dev/null) || true
    rm -f "$tmpfile" "$selected_tmpfile"
  elif command -v fzf >/dev/null 2>&1; then
    # fzf: fuzzy search + multi-select (good fallback)
    # Use temp file to avoid subshell TTY issues
    local tmpfile
    tmpfile=$(mktemp)
    echo "$display_lines" > "$tmpfile"

    local selected_tmpfile
    selected_tmpfile=$(mktemp)

    fzf --multi \
      --header="Select models for $provider (tab=toggle, enter=confirm)" \
      --height=20 \
      --reverse < "$tmpfile" > "$selected_tmpfile" 2>/dev/null || true

    selected_ids=$(awk '{print $1}' "$selected_tmpfile" 2>/dev/null) || true
    rm -f "$tmpfile" "$selected_tmpfile"
  fi

  # Fallback: simple numbered list if no TUI tool or nothing selected
  if [[ -z "$selected_ids" && "$NONINTERACTIVE" != "true" ]]; then
    echo "  Available models:"
    local i=1
    while IFS=$'\t' read -r mid name cost ctx reasoning rel; do
      local marker=" "
      if echo "$default_ids" | grep -qx "$mid"; then
        marker="*"
      fi
      printf "  %s %2d) %-40s  %s  %-12s  ctx=%-6s  %s\n" "$marker" "$i" "$mid" "$reasoning" "$cost" "$ctx" "$rel"
      i=$((i + 1))
    done <<<"$model_lines"
    echo
    echo "  Enter model numbers to select (comma/space separated, * = pre-selected)."
    echo "  Press Enter to accept defaults (top $DEFAULT_MODEL_COUNT)."
    local selection
    read -r -p "  Selection: " selection

    if [[ -z "$selection" ]]; then
      # Accept defaults
      selected_ids="$default_ids"
    else
      # Parse comma/space separated numbers
      selected_ids=""
      local nums
      nums=$(echo "$selection" | tr ',' ' ')
      for num in $nums; do
        local sel_id
        sel_id=$(echo "$model_lines" | sed -n "${num}p" | cut -f1)
        if [[ -n "$sel_id" ]]; then
          selected_ids+="$sel_id"$'\n'
        fi
      done
    fi
  fi

  if [[ -z "$selected_ids" ]]; then
    log_warn "No models selected for $provider"
    return
  fi

  # Convert selected model IDs to TOML shortlist entries using jq
  local toml_output=""
  while IFS= read -r model_id; do
    [[ -z "$model_id" ]] && continue
    # Look up full model data from catalog JSON and format as TOML
    local model_toml
    model_toml=$(echo "$catalog_json" | jq -r --arg id "$model_id" --arg prov "$provider" '
      .[] | select(.id == $id) |
      # Filter input modalities to text/image for Pi compatibility
      ([.modalities.input[]? | select(. == "text" or . == "image")] | if length == 0 then ["text"] else . end) as $input |
      "
[[providers.\($prov).models]]
id = \"\(.id)\"
name = \"\(.name // .id)\"
reasoning = \(if .reasoning then "true" else "false" end)
input = [\($input | map("\"" + . + "\"") | join(", "))]
context_window = \(.limit.context // 128000)
max_tokens = \(.limit.output // 8192)
cost = { input = \(.cost.input // 0), output = \(.cost.output // 0), cache_read = \(.cost.cache_read // 0) }"
    ' 2>/dev/null) || true
    toml_output+="$model_toml"
  done <<<"$selected_ids"

  local selected_count
  selected_count=$(echo "$selected_ids" | grep -c '.' || echo 0)
  log_success "Selected $selected_count models for $provider"
  _SELECT_MODELS_RESULT="$toml_output"
}

# ==============================================================================
# EAVS Provider Testing
# ==============================================================================
# Tests each configured provider by making a real API call via `eavs setup test`.
# This validates that API keys are correct and providers are reachable.

test_eavs_providers() {
  log_step "Testing LLM provider connections"

  local eavs_config_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_config_file="${XDG_CONFIG_HOME}/eavs/config.toml"
  else
    eavs_config_file="${OQTO_HOME}/.config/eavs/config.toml"
  fi

  # Resolve env file for the test command
  local eavs_env_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_env_file="${XDG_CONFIG_HOME}/eavs/env"
  else
    eavs_env_file="${OQTO_HOME}/.config/eavs/env"
  fi

  if [[ -z "${CONFIGURED_PROVIDERS// }" ]]; then
    log_warn "No providers configured. Skipping provider tests."
    return 0
  fi

  read_env_file() {
    if [[ "$SELECTED_USER_MODE" == "single" ]]; then
      cat "$eavs_env_file" 2>/dev/null || true
    else
      sudo cat "$eavs_env_file" 2>/dev/null || true
    fi
  }

  get_provider_api_key_ref() {
    local provider_name="$1"
    awk -v p="$provider_name" '
      $0 ~ "^\\[providers\\."p"\\]" {in=1; next}
      $0 ~ "^\\[providers\\." && $0 !~ "^\\[providers\\."p"\\]" {in=0}
      in && $0 ~ "^api_key" {
        sub(/^[^=]*= */, "", $0)
        gsub(/\"/, "", $0)
        print $0
        exit
      }
    ' "$eavs_config_file" 2>/dev/null
  }

  get_env_value() {
    local env_name="$1"
    local env_val="${!env_name:-}"
    if [[ -n "$env_val" ]]; then
      echo "$env_val"
      return
    fi
    read_env_file | sed -n "s/^${env_name}=//p" | head -1
  }

  local any_success="false"
  local any_failure="false"
  local summary_lines=()

  for provider in $CONFIGURED_PROVIDERS; do
    [[ -z "$provider" ]] && continue
    echo -n "  Testing ${provider}... "

    local api_key_ref
    api_key_ref=$(get_provider_api_key_ref "$provider")
    if [[ "$api_key_ref" == env:* ]]; then
      local env_name="${api_key_ref#env:}"
      local env_val
      env_val=$(get_env_value "$env_name")
      if [[ -z "$env_val" ]]; then
        echo -e "${YELLOW}SKIPPED${NC}"
        echo "    Missing env var ${env_name} for ${provider}"
        summary_lines+=("${provider}|SKIPPED|Missing env var ${env_name}")
        any_failure="true"
        continue
      fi
    fi

    # Source the env file so eavs setup test can resolve env: keys.
    # Redirect stdin from /dev/null so eavs doesn't try to prompt for input.
    local test_result
    if [[ "$SELECTED_USER_MODE" == "single" ]]; then
      test_result=$(
        set -a
        source "$eavs_env_file" 2>/dev/null
        set +a
        eavs setup test "$provider" --config "$eavs_config_file" --format json </dev/null 2>&1
      ) || true
    else
      # Ensure oqto can read the env file and config.
      sudo chown -R oqto:oqto "$(dirname "$eavs_config_file")" 2>/dev/null
      # Source env file in the current shell first, then pass vars via sudo env.
      local env_args=""
      if [[ -f "$eavs_env_file" ]]; then
        while IFS='=' read -r key value; do
          [[ -z "$key" || "$key" == \#* ]] && continue
          env_args+="$key=$value "
        done < <(sudo cat "$eavs_env_file" 2>/dev/null)
      fi
      test_result=$(sudo -u oqto env $env_args \
        eavs setup test "$provider" --config "$eavs_config_file" --format json </dev/null 2>&1) || true
    fi

    if echo "$test_result" | grep -qE '"success"[[:space:]]*:[[:space:]]*true|test successful'; then
      echo -e "${GREEN}OK${NC}"
      summary_lines+=("${provider}|OK|")
      any_success="true"
    else
      echo -e "${RED}FAILED${NC}"
      # Show a brief error hint
      local err_hint
      err_hint=$(echo "$test_result" | grep -i "error\|unauthorized\|invalid\|403\|401" | head -1)
      if [[ -n "$err_hint" ]]; then
        echo "    $err_hint"
      fi
      summary_lines+=("${provider}|FAILED|${err_hint}")
      any_failure="true"
    fi
  done

  echo
  log_info "Provider test summary:"
  for entry in "${summary_lines[@]}"; do
    IFS='|' read -r provider status detail <<<"$entry"
    printf "  %-20s %s\n" "$provider" "$status"
    if [[ -n "$detail" ]]; then
      echo "    ${detail}"
    fi
  done

  if [[ "$any_success" == "true" ]]; then
    log_success "At least one provider is working"
  fi
  if [[ "$any_failure" == "true" ]]; then
    log_warn "Some providers failed. You can fix API keys later in the eavs config."
    if [[ "$any_success" != "true" ]]; then
      log_warn "No working providers! Agents will not be able to use any LLM."
      log_info "Fix provider config: edit $(
        if [[ "$SELECTED_USER_MODE" == "single" ]]; then
          echo "${XDG_CONFIG_HOME}/eavs/config.toml"
        else
          echo "${OQTO_HOME}/.config/eavs/config.toml"
        fi
      )"
    fi
  fi
}

# ==============================================================================
# EAVS models.json Generation
# ==============================================================================
# Uses `eavs models export pi` to generate Pi-compatible models.json.
# The export command reads the eavs config, resolves model shortlists
# (or full catalog), and outputs the correct Pi format natively.
#
# In single-user mode: writes to ~/.pi/agent/models.json
# In multi-user mode: the oqto backend handles this at user creation time
#   (via provision_eavs_for_user in admin.rs), but we also generate a
#   template for the installing admin user.

generate_eavs_models_json() {
  log_step "Generating models.json from EAVS"

  local eavs_url="http://127.0.0.1:${EAVS_PORT}"
  local eavs_config_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_config_file="${XDG_CONFIG_HOME}/eavs/config.toml"
  else
    eavs_config_file="${OQTO_HOME}/.config/eavs/config.toml"
  fi

  # Check that eavs supports the export command (>= 0.5.5)
  if ! eavs models export --help >/dev/null 2>&1; then
    log_warn "eavs does not support 'models export' (upgrade to >= 0.5.5)"
    log_info "Skipping models.json generation. Update eavs and re-run setup."
    return 0
  fi

  # Generate Pi models.json via native eavs export
  # Use --merge if a models.json already exists to preserve non-eavs providers
  local pi_models_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    pi_models_file="$HOME/.pi/agent/models.json"
  else
    pi_models_file="${OQTO_DATA_DIR:-$HOME/.local/share/oqto}/models.json.template"
  fi

  local merge_flag=""
  if [[ -f "$pi_models_file" ]]; then
    merge_flag="--merge $pi_models_file"
    log_info "Merging into existing $pi_models_file (preserving non-eavs providers)"
  fi

  local models_json=""
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    # shellcheck disable=SC2086
    models_json=$(eavs models export pi \
      --base-url "$eavs_url" \
      --config "$eavs_config_file" \
      $merge_flag 2>/dev/null) || true
  else
    # shellcheck disable=SC2086
    models_json=$(sudo -u oqto eavs models export pi \
      --base-url "$eavs_url" \
      --config "$eavs_config_file" \
      $merge_flag 2>/dev/null) || true
  fi

  if [[ -z "$models_json" || "$models_json" == '{"providers":{}}' ]]; then
    log_warn "No providers with Pi-compatible APIs found. Skipping models.json."
    return 0
  fi

  # Write models.json
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    local pi_agent_dir="$HOME/.pi/agent"
    mkdir -p "$pi_agent_dir"
    echo "$models_json" >"${pi_agent_dir}/models.json"
    log_success "Wrote models.json to ${pi_agent_dir}/models.json"

    # Also create eavs.env for Pi to use (with a virtual key)
    provision_eavs_user_key "$(whoami)" "$HOME"
  else
    # In multi-user mode, write a template that the oqto backend will
    # use when provisioning new users. Also set up the admin user.
    local octo_data="${OQTO_DATA_DIR:-$HOME/.local/share/oqto}"
    mkdir -p "$octo_data"
    echo "$models_json" >"${octo_data}/models.json.template"
    log_success "Wrote models.json template to ${octo_data}/models.json.template"
    log_info "The oqto backend will generate per-user models.json on user creation."
  fi

  # Count total models and providers using jq (available on all target systems)
  local model_count provider_count
  model_count=$(echo "$models_json" | jq '[.providers[].models | length] | add // 0' 2>/dev/null || echo "?")
  provider_count=$(echo "$models_json" | jq '.providers | length' 2>/dev/null || echo "?")

  log_success "Models available: $model_count across $provider_count provider(s)"
}

# ==============================================================================
# EAVS User Key Provisioning
# ==============================================================================
# Creates a virtual API key for a user and writes eavs.env so Pi can
# authenticate against the eavs proxy.

provision_eavs_user_key() {
  local username="$1"
  local user_home="$2"

  local eavs_url="http://127.0.0.1:${EAVS_PORT}"

  # Create virtual key via eavs API
  local key_response
  key_response=$(curl -sf -X POST "${eavs_url}/admin/keys" \
    -H "Authorization: Bearer ${EAVS_MASTER_KEY}" \
    -H "Content-Type: application/json" \
    -d "{
      \"name\": \"oqto-user-${username}\",
      \"permissions\": {
        \"rpm_limit\": 120,
        \"max_budget_usd\": 500.0
      }
    }" 2>&1)

  if [[ -z "$key_response" ]]; then
    log_warn "Failed to create EAVS virtual key for ${username}"
    log_info "Users can still use EAVS with the master key for now."
    # Fall back to master key
    local octo_config_dir="${user_home}/.config/oqto"
    mkdir -p "$octo_config_dir"
    cat >"${octo_config_dir}/eavs.env" <<EOF
EAVS_API_KEY=${EAVS_MASTER_KEY}
EAVS_URL=${eavs_url}
EOF
    chmod 600 "${octo_config_dir}/eavs.env"
    return
  fi

  local api_key
  api_key=$(echo "$key_response" | jq -r '.key // empty' 2>/dev/null || echo "")

  if [[ -z "$api_key" ]]; then
    log_warn "Could not parse EAVS key response. Using master key as fallback."
    api_key="$EAVS_MASTER_KEY"
  fi

  local octo_config_dir="${user_home}/.config/oqto"
  mkdir -p "$octo_config_dir"
  cat >"${octo_config_dir}/eavs.env" <<EOF
EAVS_API_KEY=${api_key}
EAVS_URL=${eavs_url}
EOF
  chmod 600 "${octo_config_dir}/eavs.env"
  log_success "EAVS key provisioned for ${username}"
}

install_eavs_service() {
  log_step "Setting up EAVS service"

  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    # Single-user: systemd user service, runs as installing user
    local eavs_config_dir="${XDG_CONFIG_HOME}/eavs"
    local service_dir="$HOME/.config/systemd/user"
    mkdir -p "$service_dir"

    # Find eavs binary
    local eavs_bin="/usr/local/bin/eavs"
    [[ -x "$HOME/.cargo/bin/eavs" ]] && eavs_bin="$HOME/.cargo/bin/eavs"
    [[ -x "$HOME/.local/bin/eavs" ]] && eavs_bin="$HOME/.local/bin/eavs"
    command_exists eavs && eavs_bin="$(command -v eavs)"

    cat >"${service_dir}/eavs.service" <<EOF
[Unit]
Description=EAVS LLM Proxy
After=default.target

[Service]
Type=simple
Environment=PATH=%h/.cargo/bin:%h/.local/bin:/usr/local/bin:/usr/bin:/bin
EnvironmentFile=-${eavs_config_dir}/env
ExecStart=${eavs_bin} serve
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    systemctl --user daemon-reload
    systemctl --user enable eavs
    systemctl --user start eavs
    log_success "EAVS started (user service on port ${EAVS_PORT})"

  else
    # Multi-user: system service, runs as oqto user alongside the backend.
    # EAVS config lives in oqto's home (~oqto/.config/eavs/) so XDG just works.
    #
    # The DBUS_SESSION_BUS_ADDRESS and XDG_RUNTIME_DIR environment variables
    # give the service access to the oqto user's D-Bus session bus, which is
    # needed for gnome-keyring (the keychain: config syntax). This requires
    # linger to be enabled for oqto (done in enable_keyring_for_octo_user).
    local octo_uid
    octo_uid=$(id -u oqto)

    sudo tee /etc/systemd/system/eavs.service >/dev/null <<EOF
[Unit]
Description=EAVS LLM Proxy
After=network.target
Before=oqto.service

[Service]
Type=simple
User=oqto
Group=oqto
WorkingDirectory=${OQTO_HOME}
Environment=HOME=${OQTO_HOME}
Environment=XDG_CONFIG_HOME=${OQTO_HOME}/.config
Environment=XDG_DATA_HOME=${OQTO_HOME}/.local/share
Environment=XDG_STATE_HOME=${OQTO_HOME}/.local/state
Environment=XDG_RUNTIME_DIR=/run/user/${octo_uid}
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${octo_uid}/bus
EnvironmentFile=-${OQTO_HOME}/.config/eavs/env
ExecStartPre=+/bin/bash -c 'mkdir -p /run/user/${octo_uid} && chown oqto:oqto /run/user/${octo_uid} && chmod 700 /run/user/${octo_uid}'
ExecStart=/usr/local/bin/eavs serve
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=full
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

    sudo systemctl daemon-reload
    sudo systemctl enable eavs
    sudo systemctl start eavs
    log_success "EAVS started (system service, user=oqto, port ${EAVS_PORT})"
  fi
}

