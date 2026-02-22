# ==============================================================================
# Update Mode
# ==============================================================================
# Pulls latest code, rebuilds everything, deploys, copies config, restarts.

update_octo() {
  log_step "Updating Oqto"

  cd "$SCRIPT_DIR"

  # Pull latest code
  log_info "Pulling latest code..."
  if git pull --ff-only; then
    log_success "Code updated"
  else
    log_warn "git pull --ff-only failed, trying rebase..."
    git pull --rebase || {
      log_error "Failed to pull latest code. Resolve conflicts and retry."
      return 1
    }
    log_success "Code updated (rebased)"
  fi

  # Upgrade external tools to versions in dependencies.toml
  update_tools

  # Rebuild Oqto (backend + frontend + deploy)
  build_octo

  # Copy config to service user (multi-user mode uses /var/lib/oqto)
  if [[ -f "/var/lib/oqto/.config/oqto/config.toml" ]]; then
    log_info "Syncing config to service user..."
    local octo_config_home="/var/lib/oqto/.config/oqto"
    sudo cp "${XDG_CONFIG_HOME:-$HOME/.config}/oqto/config.toml" "${octo_config_home}/config.toml"
    if [[ -f "${XDG_CONFIG_HOME:-$HOME/.config}/oqto/env" ]]; then
      sudo cp "${XDG_CONFIG_HOME:-$HOME/.config}/oqto/env" "${octo_config_home}/env"
      sudo chmod 600 "${octo_config_home}/env"
    fi
    sudo chown -R oqto:oqto "$octo_config_home"
    log_success "Config synced"
  fi

  # Regenerate models.json (picks up new eavs providers/models)
  update_models_json

  # Restart services
  log_info "Restarting services..."
  if sudo systemctl is-active --quiet eavs 2>/dev/null; then
    sudo systemctl restart eavs
    log_success "eavs restarted"
  fi
  if sudo systemctl is-active --quiet oqto 2>/dev/null; then
    sudo systemctl restart oqto
    log_success "oqto service restarted"
  elif systemctl --user is-active --quiet oqto 2>/dev/null; then
    systemctl --user restart oqto
    log_success "oqto service restarted"
  fi

  if sudo systemctl is-active --quiet caddy 2>/dev/null; then
    sudo systemctl restart caddy
    log_success "caddy restarted"
  fi

  # Quick health check
  sleep 2
  if curl -sf http://localhost:8080/api/health >/dev/null 2>&1; then
    log_success "Backend is healthy"
  else
    log_warn "Backend health check failed - check logs with: sudo journalctl -u oqto -n 20"
  fi

  log_success "Update complete!"
}

# Upgrade external tools to the versions tracked in dependencies.toml.
# Only upgrades tools that are already installed (doesn't install new ones).
# Skips tools already at the target version.
update_tools() {
  log_step "Upgrading external tools"

  # Tools that setup.sh manages (binary name -> repo name)
  local -A TOOLS=(
    [eavs]=eavs
    [hstry]=hstry
    [mmry]=mmry
    [trx]=trx
    [agntz]=agntz
    [mailz]=mailz
    [sx]=sx
    [scrpr]=scrpr
    [tmpltr]=tmpltr
    [ignr]=ignr
  )

  # Package names for multi-binary repos (binary -> cargo package)
  local -A PACKAGES=(
    [hstry]=hstry-cli
    [trx]=trx-cli
    [mmry]=mmry-cli
  )

  local upgraded=0

  for tool in "${!TOOLS[@]}"; do
    # Skip tools not currently installed
    if ! command_exists "$tool"; then
      continue
    fi

    local repo="${TOOLS[$tool]}"
    local target_version
    target_version=$(get_dep_version "$repo")
    [[ -z "$target_version" || "$target_version" == "latest" ]] && continue

    # Get currently installed version
    local current_version
    current_version=$("$tool" --version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "unknown")

    if [[ "$current_version" == "$target_version" ]]; then
      log_info "$tool $current_version (up to date)"
      continue
    fi

    log_info "Upgrading $tool: $current_version -> $target_version"
    local pkg="${PACKAGES[$tool]:-}"
    download_or_build_tool "$tool" "$repo" "$pkg"
    ((upgraded++)) || true
  done

  # Special: eavs adapters (must be installed alongside the binary)
  if command_exists eavs; then
    install_eavs_adapters
  fi

  if ((upgraded > 0)); then
    log_success "Upgraded $upgraded tool(s)"
  else
    log_info "All tools up to date"
  fi
}

# Regenerate models.json without interactive prompts.
# Used by --update to pick up new models/providers from eavs config changes.
update_models_json() {
  # Need eavs with export support
  if ! command_exists eavs || ! eavs models export --help >/dev/null 2>&1; then
    return 0
  fi

  local eavs_config_file
  local eavs_url="http://127.0.0.1:${EAVS_PORT:-3033}"

  # Detect mode from existing config
  if [[ -f "/var/lib/oqto/.config/oqto/config.toml" ]]; then
    # Multi-user
    eavs_config_file="/var/lib/oqto/.config/eavs/config.toml"
    if [[ ! -f "$eavs_config_file" ]]; then
      eavs_config_file="${XDG_CONFIG_HOME:-$HOME/.config}/eavs/config.toml"
    fi

    local pi_models_file="$HOME/.pi/agent/models.json"
    local merge_flag=""
    if [[ -f "$pi_models_file" ]]; then
      merge_flag="--merge $pi_models_file"
    fi

    local models_json
    # shellcheck disable=SC2086
    models_json=$(eavs models export pi \
      --base-url "$eavs_url" \
      --config "$eavs_config_file" \
      $merge_flag 2>/dev/null) || true

    if [[ -n "$models_json" && "$models_json" != '{"providers":{}}' ]]; then
      mkdir -p "$HOME/.pi/agent"
      echo "$models_json" >"$HOME/.pi/agent/models.json"
      local count
      count=$(echo "$models_json" | jq '[.providers[].models | length] | add // 0' 2>/dev/null || echo "?")
      log_success "models.json updated ($count models)"
    fi
  else
    # Single-user
    eavs_config_file="${XDG_CONFIG_HOME:-$HOME/.config}/eavs/config.toml"
    local pi_models_file="$HOME/.pi/agent/models.json"
    local merge_flag=""
    if [[ -f "$pi_models_file" ]]; then
      merge_flag="--merge $pi_models_file"
    fi

    local models_json
    # shellcheck disable=SC2086
    models_json=$(eavs models export pi \
      --base-url "$eavs_url" \
      --config "$eavs_config_file" \
      $merge_flag 2>/dev/null) || true

    if [[ -n "$models_json" && "$models_json" != '{"providers":{}}' ]]; then
      mkdir -p "$HOME/.pi/agent"
      echo "$models_json" >"$HOME/.pi/agent/models.json"
      local count
      count=$(echo "$models_json" | jq '[.providers[].models | length] | add // 0' 2>/dev/null || echo "?")
      log_success "models.json updated ($count models)"
    fi
  fi
}

# Download pre-built oqto binaries and frontend from GitHub releases.
# Returns 0 on success, 1 if download fails (caller should fall back to source build).
download_oqto_release() {
  local target
  target=$(get_release_target)
  if [[ -z "$target" ]]; then
    log_info "Could not detect platform target"
    return 1
  fi

  # Determine version: use git tag if we're on one, otherwise latest release
  local version=""
  if git describe --tags --exact-match HEAD 2>/dev/null | grep -q '^v'; then
    version=$(git describe --tags --exact-match HEAD 2>/dev/null)
  fi
  if [[ -z "$version" ]]; then
    # Try to get latest release tag from GitHub
    version=$(curl -fsSL "https://api.github.com/repos/byteowlz/oqto/releases/latest" 2>/dev/null \
      | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
  fi
  if [[ -z "$version" ]]; then
    log_info "Could not determine release version"
    return 1
  fi

  log_info "Downloading oqto $version for $target..."

  local tmpdir
  tmpdir=$(mktemp -d)
  local base_url="https://github.com/byteowlz/oqto/releases/download/${version}"

  # Download backend binaries
  local backend_tarball="oqto-${version}-${target}.tar.gz"
  if ! curl -fsSL "${base_url}/${backend_tarball}" -o "$tmpdir/backend.tar.gz" 2>/dev/null; then
    log_info "Backend release not found at ${base_url}/${backend_tarball}"
    rm -rf "$tmpdir"
    return 1
  fi

  # Download frontend
  local frontend_tarball="oqto-frontend-${version}.tar.gz"
  if ! curl -fsSL "${base_url}/${frontend_tarball}" -o "$tmpdir/frontend.tar.gz" 2>/dev/null; then
    log_info "Frontend release not found at ${base_url}/${frontend_tarball}"
    rm -rf "$tmpdir"
    return 1
  fi

  # Extract backend
  tar xzf "$tmpdir/backend.tar.gz" -C "$tmpdir"
  local backend_dir="$tmpdir/oqto-${version}-${target}"
  if [[ ! -d "$backend_dir/bin" ]]; then
    log_warn "Release tarball missing bin/ directory"
    rm -rf "$tmpdir"
    return 1
  fi

  # Install backend binaries
  local installed=0
  for bin in "$backend_dir"/bin/*; do
    if [[ -x "$bin" ]]; then
      local name
      name=$(basename "$bin")
      sudo install -m 755 "$bin" "${TOOLS_INSTALL_DIR}/${name}"
      log_success "${name} installed from release"
      installed=$((installed + 1))
    fi
  done

  if [[ $installed -eq 0 ]]; then
    log_warn "No binaries found in release"
    rm -rf "$tmpdir"
    return 1
  fi

  # Extract and deploy frontend
  tar xzf "$tmpdir/frontend.tar.gz" -C "$tmpdir"
  local frontend_dir="$tmpdir/oqto-frontend-${version}"
  if [[ -d "$frontend_dir/dist" ]]; then
    local frontend_deploy="/var/www/oqto"
    sudo mkdir -p "$frontend_deploy"
    sudo rsync -a --delete "$frontend_dir/dist/" "$frontend_deploy/"
    sudo chown -R root:root "$frontend_deploy"
    log_success "Frontend deployed from release to ${frontend_deploy}"
  else
    log_warn "Frontend dist not found in release, will build from source"
  fi

  rm -rf "$tmpdir"
  log_success "oqto $version installed from GitHub release ($installed binaries)"
  return 0
}

# Build oqto backend and frontend from source.
build_oqto_from_source() {
  # Build backend (includes oqto, oqto-runner, oqto-sandbox, pi-bridge binaries)
  log_info "Building backend..."
  if ! (cd backend && cargo build --release); then
    log_error "Backend build failed"
    return 1
  fi
  log_success "Backend built"

  # Build fileserver (oqto-files crate in workspace)
  log_info "Building fileserver..."
  if ! (cd backend && cargo build --release -p oqto-files --bin oqto-files); then
    log_error "Fileserver build failed"
    return 1
  fi
  log_success "Fileserver built"

  # Build frontend
  log_info "Installing frontend dependencies..."
  (cd frontend && bun install)
  log_info "Building frontend..."
  if ! (cd frontend && bun run build); then
    log_error "Frontend build failed"
    return 1
  fi
  log_success "Frontend built"
}

build_octo() {
  log_step "Building Oqto components"

  cd "$SCRIPT_DIR"

  # Clean up stale directories from octo->oqto rename that confuse workspace
  rm -rf backend/crates/octo-browserd backend/crates/octo-browser backend/crates/octo 2>/dev/null || true

  # Try downloading pre-built release binaries from GitHub
  local used_release=false
  if download_oqto_release; then
    log_success "Using pre-built release binaries"
    used_release=true
  else
    # Fall back to building from source
    log_info "No pre-built release available, building from source..."
    build_oqto_from_source
  fi

  # Build and install agent browser daemon
  log_info "Building agent browser daemon..."
  local browserd_dir="$SCRIPT_DIR/backend/crates/oqto-browserd"
  if [[ -d "$browserd_dir" ]]; then
    (cd "$browserd_dir" && bun install && bun run build)
    local browserd_deploy="/usr/local/lib/oqto-browserd"
    sudo mkdir -p "$browserd_deploy/bin" "$browserd_deploy/dist"
    sudo cp "$browserd_dir/bin/oqto-browserd.js" "$browserd_deploy/bin/"
    sudo cp "$browserd_dir/dist/"*.js "$browserd_deploy/dist/"
    sudo cp "$browserd_dir/package.json" "$browserd_deploy/"
    (cd "$browserd_deploy" && sudo bun install --production)
    # Install Playwright browsers to a shared location accessible by all users
    local pw_browsers="/usr/local/share/playwright-browsers"
    sudo mkdir -p "$pw_browsers"
    sudo chmod 755 "$pw_browsers"
    log_info "Installing Playwright chromium to $pw_browsers..."
    sudo PLAYWRIGHT_BROWSERS_PATH="$pw_browsers" npx --yes playwright install --with-deps chromium 2>/dev/null ||
      sudo PLAYWRIGHT_BROWSERS_PATH="$pw_browsers" bunx playwright install --with-deps chromium 2>/dev/null ||
      log_warn "Playwright browser install failed - agent browser may not work"
    log_success "Agent browser daemon installed to $browserd_deploy"
  else
    log_warn "oqto-browserd not found, skipping agent browser setup"
  fi

  # When built from source, install binaries and deploy frontend
  if [[ "$used_release" != "true" ]]; then
    # Install binaries to /usr/local/bin (globally accessible)
    log_info "Installing binaries to ${TOOLS_INSTALL_DIR}..."

    local release_dir="$SCRIPT_DIR/backend/target/release"
    for bin in oqto oqtoctl oqto-runner oqto-browser pi-bridge oqto-sandbox oqto-setup oqto-usermgr; do
      if [[ -f "${release_dir}/${bin}" ]]; then
        sudo install -m 755 "${release_dir}/${bin}" "${TOOLS_INSTALL_DIR}/${bin}"
        # Remove stale copies from ~/.cargo/bin to avoid PATH precedence issues
        if [[ -f "$HOME/.cargo/bin/${bin}" ]]; then
          rm -f "$HOME/.cargo/bin/${bin}"
          hash -d "$bin" 2>/dev/null || true
          log_info "Removed stale ${bin} from ~/.cargo/bin"
        fi
        log_success "${bin} installed"
      fi
    done

    if [[ -f "$SCRIPT_DIR/backend/target/release/oqto-files" ]]; then
      sudo install -m 755 "$SCRIPT_DIR/backend/target/release/oqto-files" "${TOOLS_INSTALL_DIR}/oqto-files"
      log_success "oqto-files installed"
    fi

    log_success "Binaries installed to ${TOOLS_INSTALL_DIR}"

    # Deploy frontend static files
    local frontend_dist="$SCRIPT_DIR/frontend/dist"
    local frontend_deploy="/var/www/oqto"
    if [[ -d "$frontend_dist" ]]; then
      log_info "Deploying frontend to ${frontend_deploy}..."
      sudo mkdir -p "$frontend_deploy"
      sudo rsync -a --delete "$frontend_dist/" "$frontend_deploy/"
      sudo chown -R root:root "$frontend_deploy"
      log_success "Frontend deployed to ${frontend_deploy}"
    else
      log_warn "Frontend dist not found, skipping deployment"
    fi
  fi

  # Restart running services so they pick up the new binaries
  if sudo systemctl is-active --quiet oqto-usermgr 2>/dev/null; then
    sudo systemctl restart oqto-usermgr
    log_success "oqto-usermgr restarted"
  fi
  if sudo systemctl is-active --quiet oqto 2>/dev/null; then
    sudo systemctl restart oqto
    log_success "oqto restarted with new binary"
  elif systemctl --user is-active --quiet oqto 2>/dev/null; then
    # Single-user mode: restart runner first, then backend
    if systemctl --user is-active --quiet oqto-runner 2>/dev/null; then
      systemctl --user restart oqto-runner
      sleep 2
      log_success "oqto-runner restarted with new binary"
    fi
    systemctl --user restart oqto
    log_success "oqto restarted with new binary"
  fi
  if sudo systemctl is-active --quiet eavs 2>/dev/null; then
    sudo systemctl restart eavs
    log_success "eavs restarted"
  fi
}

