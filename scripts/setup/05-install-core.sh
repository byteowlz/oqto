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

validate_pi_rpc_smoke() {
  local pi_bin="$1"
  local tmp_dir fifo out err pid
  tmp_dir="$(mktemp -d)"
  fifo="$tmp_dir/stdin"
  out="$tmp_dir/stdout"
  err="$tmp_dir/stderr"
  mkfifo "$fifo"

  "$pi_bin" --mode rpc <"$fifo" >"$out" 2>"$err" &
  pid=$!

  exec 9>"$fifo"
  sleep 2

  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    exec 9>&-
    rm -rf "$tmp_dir"
    return 0
  fi

  wait "$pid" 2>/dev/null || true
  exec 9>&-
  log_error "Pi RPC smoke test failed for $pi_bin"
  if [[ -s "$err" ]]; then
    sed 's/^/  stderr: /' "$err" >&2 || true
  fi
  rm -rf "$tmp_dir"
  return 1
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

  # Install/update pi (system-wide) to the pinned version via the shared
  # agent-runtime sync — the SAME script deploy runs, so setup and deploy can't
  # drift. It handles the bun-global install, the /usr/local/lib copy, the
  # self-links (so extensions resolve the host package), and the /usr/local/bin/pi
  # wrapper. Pin comes from dependencies.toml.
  if ! "${SCRIPT_DIR}/scripts/dist/sync-agent-runtime.sh" --skip-extensions; then
    log_warn "pi sync failed; pi may not be globally accessible."
    return 0
  fi
  validate_pi_rpc_smoke /usr/local/bin/pi
  log_success "pi installed system-wide: $(/usr/local/bin/pi --version 2>/dev/null || echo 'installed')"
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

