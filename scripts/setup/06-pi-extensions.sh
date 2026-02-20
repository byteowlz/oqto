# ==============================================================================
# Pi Extensions Installation
# ==============================================================================

# GitHub repo for Pi agent extensions (SSH for private repo access)
PI_EXTENSIONS_REPO="https://github.com/byteowlz/pi-agent-extensions.git"

# Default extensions to install (subset of what's available in the repo)
# These are the Oqto-relevant extensions; users can install others via the
# pi-agent-extensions justfile.
PI_DEFAULT_EXTENSIONS=(
  "auto-rename"
  "oqto-bridge"
  "oqto-todos"
  "custom-context-files"
)

# Clone or update the pi-agent-extensions repo into a cache directory.
# Falls back to a local checkout if available.
clone_pi_extensions_repo() {
  # Check for local checkout first (faster, works offline)
  for base in "$HOME/byteowlz" "$HOME/code/byteowlz" "/opt/byteowlz"; do
    local local_dir="$base/pi-agent-extensions"
    if [[ -d "$local_dir" && -f "$local_dir/README.md" ]]; then
      log_info "Using local pi-agent-extensions: $local_dir" >&2
      echo "$local_dir"
      return 0
    fi
  done

  local cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/oqto/pi-agent-extensions"

  if [[ -d "$cache_dir/.git" ]]; then
    log_info "Updating pi-agent-extensions repo..." >&2
    # Ensure remote URL is HTTPS (may have been cloned via SSH previously)
    git -C "$cache_dir" remote set-url origin "$PI_EXTENSIONS_REPO" >/dev/null 2>&1 || true
    if git -C "$cache_dir" fetch origin >/dev/null 2>&1; then
      git -C "$cache_dir" reset --hard origin/main >/dev/null 2>&1 ||
        git -C "$cache_dir" reset --hard origin/HEAD >/dev/null 2>&1 || true
    fi
  else
    # Remove stale cache dir if it exists without .git
    if [[ -d "$cache_dir" ]]; then
      rm -rf "$cache_dir"
    fi
    log_info "Cloning pi-agent-extensions..." >&2
    mkdir -p "$(dirname "$cache_dir")"
    if ! git clone --depth 1 "$PI_EXTENSIONS_REPO" "$cache_dir" >/dev/null 2>&1; then
      log_error "Failed to clone pi-agent-extensions repo" >&2
      return 1
    fi
  fi

  # Verify the clone has content (an extension with index.ts should exist)
  if [[ ! -f "$cache_dir/oqto-bridge/index.ts" ]]; then
    log_warn "pi-agent-extensions cache is stale or broken, re-cloning..." >&2
    rm -rf "$cache_dir"
    mkdir -p "$(dirname "$cache_dir")"
    if ! git clone --depth 1 "$PI_EXTENSIONS_REPO" "$cache_dir" >/dev/null 2>&1; then
      log_error "Failed to re-clone pi-agent-extensions repo" >&2
      return 1
    fi
  fi

  echo "$cache_dir"
}

install_pi_extensions() {
  log_step "Installing Pi extensions"

  local ext_source
  ext_source=$(clone_pi_extensions_repo) || return 1

  # Install for current user
  install_pi_extensions_for_user "$HOME" "$ext_source"

  log_success "Pi extensions installed"
}

# Install Pi extensions for a specific user's home directory
# Args: $1 = user home dir, $2 = extensions source dir (cloned repo)
install_pi_extensions_for_user() {
  local user_home="$1"
  local ext_source="$2"
  local extensions_dir="${user_home}/.pi/agent/extensions"

  log_info "Installing Pi extensions to ${extensions_dir}"

  mkdir -p "$extensions_dir"

  local installed=0
  for ext_name in "${PI_DEFAULT_EXTENSIONS[@]}"; do
    local src_dir="${ext_source}/${ext_name}"
    local dest_dir="${extensions_dir}/${ext_name}"

    if [[ ! -d "$src_dir" || ! -f "$src_dir/index.ts" ]]; then
      log_warn "Extension not found in repo: $ext_name"
      continue
    fi

    # Copy extension directory
    rm -rf "$dest_dir"
    cp -r "$src_dir" "$dest_dir"

    # Remove install script (not needed at runtime); keep package.json
    # (Pi reads "pi.extensions" from it to find the entry point)
    rm -f "$dest_dir/install.sh"

    log_success "Installed $ext_name extension"
    ((installed++)) || true
  done

  # Create a README for the user
  cat >"${extensions_dir}/README.md" <<'EOF'
# Pi Agent Extensions (installed by Oqto)

These extensions are installed by Oqto setup from the pi-agent-extensions
repository: https://github.com/byteowlz/pi-agent-extensions

## Installed Extensions

- **auto-rename**: Automatically generate session names from first user query
- **oqto-bridge**: Emit granular agent phase status for the Oqto runner
- **oqto-todos**: Todo management tools for Oqto frontend integration
- **custom-context-files**: Auto-load USER.md, PERSONALITY.md, and other context files into prompts

## Managing Extensions

To install additional extensions or update existing ones, clone the
pi-agent-extensions repo and use its justfile:

    git clone https://github.com/byteowlz/pi-agent-extensions.git
    cd pi-agent-extensions
    just install      # Interactive picker
    just install-all  # Install everything
    just status       # Show sync status

Or re-run the Oqto setup script to update the default set.
EOF

  log_info "$installed extensions installed to ${extensions_dir}"
}

# Install Pi extensions for all users in multi-user mode
install_pi_extensions_all_users() {
  log_step "Installing Pi extensions for all users"

  local ext_source
  ext_source=$(clone_pi_extensions_repo) || return 1

  # Install for current user first
  install_pi_extensions_for_user "$HOME" "$ext_source"

  # Install to /etc/skel so new users get extensions automatically
  if [[ "$SELECTED_USER_MODE" == "multi" && -d "/etc/skel" ]]; then
    log_info "Installing Pi extensions to /etc/skel for new users..."
    sudo mkdir -p /etc/skel/.pi/agent/extensions
    install_pi_extensions_for_user "/etc/skel" "$ext_source" 2>/dev/null || true
  fi

  log_success "Pi extensions installed for all applicable users"
}

