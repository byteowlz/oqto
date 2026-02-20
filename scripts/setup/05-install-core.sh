# ==============================================================================
# Installation Functions
# ==============================================================================

install_rust() {
  log_info "Installing Rust via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
  log_success "Rust installed: $(cargo --version)"
}

install_bun() {
  log_info "Installing Bun..."
  curl -fsSL https://bun.sh/install | bash
  export PATH="$HOME/.bun/bin:$PATH"

  log_success "Bun installed: $(bun --version)"

  ensure_bun_and_pi_global
}

# Ensure bun and pi are globally accessible to all platform users.
# Called both after fresh install and on every setup run.
ensure_bun_and_pi_global() {
  # Always copy bun to /usr/local/bin for multi-user access.
  # Use install(1) which copies the file (not symlink) and sets permissions.
  # This also fixes broken symlinks from previous installs.
  if [[ -x "$HOME/.bun/bin/bun" ]]; then
    sudo rm -f /usr/local/bin/bun
    sudo install -m 755 "$HOME/.bun/bin/bun" /usr/local/bin/bun
    log_info "Installed bun to /usr/local/bin for multi-user access"
  fi

  # Install pi (AI coding agent) if not already present
  if ! command_exists pi || ! pi --version >/dev/null 2>&1; then
    log_info "Installing pi coding agent..."
    bun install -g @mariozechner/pi-coding-agent
  fi

  # Install pi system-wide so all platform users can run it.
  # bun global installs go to ~/.bun/install/global/ which is per-user,
  # so we copy the package to a shared location and create a wrapper.
  local pi_src_dir="$HOME/.bun/install/global/node_modules/@mariozechner/pi-coding-agent"
  local pi_system_dir="/usr/local/lib/pi-coding-agent"
  if [[ -d "$pi_src_dir" ]]; then
    # Copy the full package (with node_modules) to a system-wide location
    sudo rm -rf "$pi_system_dir"
    sudo cp -a "$pi_src_dir" "$pi_system_dir"
    sudo chmod -R a+rX "$pi_system_dir"

    # Install all dependencies into the system-wide copy so bun can resolve them
    (cd "$pi_system_dir" && sudo /usr/local/bin/bun install --frozen-lockfile 2>/dev/null || sudo /usr/local/bin/bun install 2>/dev/null) || true

    # Create wrapper that uses the system-wide copy.
    # PI_PACKAGE_DIR tells Pi where to find themes, examples, package.json.
    # Prefer user's bun (installed per-user by provisioning) over system bun.
    sudo tee /usr/local/bin/pi >/dev/null <<'PIEOF'
#!/usr/bin/env bash
PI_PKG="/usr/local/lib/pi-coding-agent"
if [ ! -f "$PI_PKG/dist/cli.js" ]; then
  echo "Error: pi-coding-agent not found at $PI_PKG" >&2
  exit 1
fi
BUN="${HOME}/.bun/bin/bun"
[ -x "$BUN" ] || BUN="/usr/local/bin/bun"
[ -x "$BUN" ] || { echo "Error: bun not found" >&2; exit 1; }
export PI_PACKAGE_DIR="$PI_PKG"
exec "$BUN" "$PI_PKG/dist/cli.js" "$@"
PIEOF
    sudo chmod 755 /usr/local/bin/pi
    log_success "pi installed system-wide: $(/usr/local/bin/pi --version 2>/dev/null || echo 'installed')"
  else
    log_warn "Could not find pi module at $pi_src_dir. Pi may not be globally accessible."
  fi
}

install_ttyd() {
  log_step "Installing ttyd (web terminal)"

  if command_exists ttyd; then
    log_success "ttyd already installed: $(ttyd --version 2>/dev/null | head -1)"
    return 0
  fi

  case "$OS" in
  macos)
    if command_exists brew; then
      log_info "Installing ttyd via Homebrew..."
      brew install ttyd
    else
      log_warn "Homebrew not found. Please install ttyd manually:"
      log_info "  brew install ttyd"
      log_info "  or download from: https://github.com/tsl0922/ttyd/releases"
    fi
    ;;
  linux)
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros)
      log_info "Installing ttyd via pacman..."
      sudo pacman -S --noconfirm ttyd
      ;;
    debian | ubuntu | pop | linuxmint)
      log_info "Installing ttyd via apt..."
      apt_update_once
      sudo apt-get install -y ttyd
      ;;
    fedora | centos | rhel | rocky | alma)
      log_info "Installing ttyd via dnf..."
      sudo dnf install -y ttyd || install_ttyd_from_source
      ;;
    opensuse* | suse*)
      log_info "Installing ttyd from binary (not in openSUSE repos)..."
      install_ttyd_from_source
      ;;
    *)
      log_warn "Unknown distribution. Attempting to download binary..."
      install_ttyd_from_source
      ;;
    esac
    ;;
  esac

  if command_exists ttyd; then
    log_success "ttyd installed successfully"
  else
    log_warn "ttyd installation may have failed. Please install manually."
  fi
}

install_ttyd_from_source() {
  local ttyd_version="1.7.7"
  local ttyd_url="https://github.com/tsl0922/ttyd/releases/download/${ttyd_version}/ttyd.$(uname -m)"

  log_info "Downloading ttyd binary..."
  sudo curl -L "$ttyd_url" -o /usr/local/bin/ttyd
  sudo chmod +x /usr/local/bin/ttyd
}

