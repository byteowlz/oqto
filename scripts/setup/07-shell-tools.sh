# ==============================================================================
# Shell Tools Installation
# ==============================================================================

ensure_fdfind_symlink() {
  if command_exists fdfind && ! command_exists fd; then
    sudo ln -sf "$(command -v fdfind)" /usr/local/bin/fd
    log_success "Created fd -> fdfind symlink"
  fi
}

install_shell_tools() {
  log_step "Installing shell tools"

  local tools_to_install=()

  # Check each tool
  if ! command_exists tmux; then
    tools_to_install+=("tmux")
  else
    log_success "tmux already installed: $(tmux -V)"
  fi

  if ! command_exists fd; then
    # fd is sometimes called fd-find on some systems
    if ! command_exists fdfind; then
      tools_to_install+=("fd")
    else
      log_success "fd already installed (as fdfind)"
      ensure_fdfind_symlink
    fi
  else
    log_success "fd already installed: $(fd --version | head -1)"
  fi

  if ! command_exists rg; then
    tools_to_install+=("ripgrep")
  else
    log_success "ripgrep already installed: $(rg --version | head -1)"
  fi

  if ! command_exists yazi; then
    tools_to_install+=("yazi")
  else
    log_success "yazi already installed: $(yazi --version 2>/dev/null || echo 'version unknown')"
  fi

  if ! command_exists zsh; then
    tools_to_install+=("zsh")
  else
    log_success "zsh already installed: $(zsh --version)"
  fi

  if ! command_exists zoxide; then
    tools_to_install+=("zoxide")
  else
    log_success "zoxide already installed: $(zoxide --version)"
  fi

  if ! command_exists gum; then
    tools_to_install+=("gum")
  else
    log_success "gum already installed: $(gum --version 2>/dev/null | head -1)"
  fi

  if ! command_exists fzf; then
    tools_to_install+=("fzf")
  else
    log_success "fzf already installed: $(fzf --version 2>/dev/null | head -1)"
  fi

  if [[ ${#tools_to_install[@]} -eq 0 ]]; then
    log_success "All shell tools already installed"
    return 0
  fi

  log_info "Tools to install: ${tools_to_install[*]}"

  if ! confirm "Install missing shell tools?"; then
    log_warn "Skipping shell tools installation"
    return 0
  fi

  case "$OS" in
  macos)
    install_shell_tools_macos "${tools_to_install[@]}"
    ;;
  linux)
    install_shell_tools_linux "${tools_to_install[@]}"
    ;;
  esac
}

install_shell_tools_macos() {
  local tools=("$@")

  if ! command_exists brew; then
    log_warn "Homebrew not found. Installing Homebrew first..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
  fi

  for tool in "${tools[@]}"; do
    case "$tool" in
    fd)
      log_info "Installing fd..."
      brew install fd
      ;;
    ripgrep)
      log_info "Installing ripgrep..."
      brew install ripgrep
      ;;
    yazi)
      log_info "Installing yazi..."
      brew install yazi
      ;;
    zoxide)
      log_info "Installing zoxide..."
      brew install zoxide
      ;;
    tmux)
      log_info "Installing tmux..."
      brew install tmux
      ;;
    zsh)
      log_info "Installing zsh..."
      brew install zsh
      ;;
    gum)
      log_info "Installing gum..."
      brew install gum
      ;;
    fzf)
      log_info "Installing fzf..."
      brew install fzf
      ;;
    esac
  done
}

install_shell_tools_linux() {
  local tools=("$@")

  case "$OS_DISTRO" in
  arch | manjaro | endeavouros)
    install_shell_tools_arch "${tools[@]}"
    ;;
  debian | ubuntu | pop | linuxmint)
    install_shell_tools_debian "${tools[@]}"
    ;;
  fedora | rhel | centos | rocky | almalinux)
    install_shell_tools_fedora "${tools[@]}"
    ;;
  opensuse*)
    install_shell_tools_opensuse "${tools[@]}"
    ;;
  *)
    log_warn "Unknown distribution: $OS_DISTRO"
    log_info "Attempting to install via cargo for Rust tools..."
    install_shell_tools_cargo "${tools[@]}"
    ;;
  esac
}

install_shell_tools_arch() {
  local tools=("$@")
  local pacman_pkgs=()

  for tool in "${tools[@]}"; do
    case "$tool" in
    fd) pacman_pkgs+=("fd") ;;
    ripgrep) pacman_pkgs+=("ripgrep") ;;
    yazi) pacman_pkgs+=("yazi") ;;
    zoxide) pacman_pkgs+=("zoxide") ;;
    tmux) pacman_pkgs+=("tmux") ;;
    zsh) pacman_pkgs+=("zsh") ;;
    gum) pacman_pkgs+=("gum") ;;
    fzf) pacman_pkgs+=("fzf") ;;
    esac
  done

  if [[ ${#pacman_pkgs[@]} -gt 0 ]]; then
    log_info "Installing via pacman: ${pacman_pkgs[*]}"
    sudo pacman -S --noconfirm "${pacman_pkgs[@]}"
  fi
}

install_shell_tools_debian() {
  local tools=("$@")
  local apt_pkgs=()
  local cargo_pkgs=()

  for tool in "${tools[@]}"; do
    case "$tool" in
    fd) apt_pkgs+=("fd-find") ;;
    ripgrep) apt_pkgs+=("ripgrep") ;;
    yazi) cargo_pkgs+=("yazi-fm") ;;  # yazi not in apt, use cargo
    zoxide) cargo_pkgs+=("zoxide") ;; # newer versions via cargo
    tmux) apt_pkgs+=("tmux") ;;
    zsh) apt_pkgs+=("zsh") ;;
    fzf) apt_pkgs+=("fzf") ;;
    gum) ;; # handled separately below (needs charmbracelet repo)
    esac
  done

  if [[ ${#apt_pkgs[@]} -gt 0 ]]; then
    log_info "Installing via apt: ${apt_pkgs[*]}"
    apt_update_once
    sudo apt-get install -y "${apt_pkgs[@]}"

    # On Debian/Ubuntu, fd-find installs as 'fdfind' - create 'fd' symlink
    if command_exists fdfind && ! command_exists fd; then
      sudo ln -sf "$(command -v fdfind)" /usr/local/bin/fd
      log_success "Created fd -> fdfind symlink"
    fi
  fi

  if [[ ${#cargo_pkgs[@]} -gt 0 ]]; then
    install_shell_tools_cargo "${cargo_pkgs[@]}"
  fi

  # gum requires the charmbracelet apt repo
  if printf '%s\n' "${tools[@]}" | grep -qx "gum"; then
    if ! command_exists gum; then
      log_info "Installing gum via charmbracelet repo..."
      sudo mkdir -p /etc/apt/keyrings
      curl -fsSL https://repo.charm.sh/apt/gpg.key | sudo gpg --dearmor -o /etc/apt/keyrings/charm.gpg 2>/dev/null
      echo "deb [signed-by=/etc/apt/keyrings/charm.gpg] https://repo.charm.sh/apt/ * *" | sudo tee /etc/apt/sources.list.d/charm.list >/dev/null
      apt_update_once force
      sudo apt-get install -y gum
    fi
  fi
}

install_shell_tools_fedora() {
  local tools=("$@")
  local dnf_pkgs=()
  local cargo_pkgs=()

  for tool in "${tools[@]}"; do
    case "$tool" in
    fd) dnf_pkgs+=("fd-find") ;;
    ripgrep) dnf_pkgs+=("ripgrep") ;;
    yazi) cargo_pkgs+=("yazi-fm") ;;
    zoxide) dnf_pkgs+=("zoxide") ;;
    tmux) dnf_pkgs+=("tmux") ;;
    zsh) dnf_pkgs+=("zsh") ;;
    fzf) dnf_pkgs+=("fzf") ;;
    gum) ;; # handled separately below
    esac
  done

  if [[ ${#dnf_pkgs[@]} -gt 0 ]]; then
    log_info "Installing via dnf: ${dnf_pkgs[*]}"
    sudo dnf install -y "${dnf_pkgs[@]}"
  fi

  if [[ ${#cargo_pkgs[@]} -gt 0 ]]; then
    install_shell_tools_cargo "${cargo_pkgs[@]}"
  fi

  # gum via charmbracelet rpm repo
  if printf '%s\n' "${tools[@]}" | grep -qx "gum"; then
    if ! command_exists gum; then
      log_info "Installing gum via charmbracelet repo..."
      echo '[charm]
name=Charm
baseurl=https://repo.charm.sh/yum/
enabled=1
gpgcheck=1
gpgkey=https://repo.charm.sh/yum/gpg.key' | sudo tee /etc/yum.repos.d/charm.repo >/dev/null
      sudo dnf install -y gum
    fi
  fi
}

install_shell_tools_opensuse() {
  local tools=("$@")
  local zypper_pkgs=()
  local cargo_pkgs=()

  for tool in "${tools[@]}"; do
    case "$tool" in
    fd) zypper_pkgs+=("fd") ;;
    ripgrep) zypper_pkgs+=("ripgrep") ;;
    yazi) cargo_pkgs+=("yazi-fm") ;;
    zoxide) cargo_pkgs+=("zoxide") ;;
    tmux) zypper_pkgs+=("tmux") ;;
    zsh) zypper_pkgs+=("zsh") ;;
    fzf) zypper_pkgs+=("fzf") ;;
    gum) ;; # handled separately below
    esac
  done

  if [[ ${#zypper_pkgs[@]} -gt 0 ]]; then
    log_info "Installing via zypper: ${zypper_pkgs[*]}"
    sudo zypper install -y "${zypper_pkgs[@]}"
  fi

  if [[ ${#cargo_pkgs[@]} -gt 0 ]]; then
    install_shell_tools_cargo "${cargo_pkgs[@]}"
  fi

  # gum via charmbracelet rpm repo
  if printf '%s\n' "${tools[@]}" | grep -qx "gum"; then
    if ! command_exists gum; then
      log_info "Installing gum via charmbracelet repo..."
      echo '[charm]
name=Charm
baseurl=https://repo.charm.sh/yum/
enabled=1
gpgcheck=1
gpgkey=https://repo.charm.sh/yum/gpg.key' | sudo tee /etc/yum.repos.d/charm.repo >/dev/null
      sudo zypper refresh
      sudo zypper install -y gum
    fi
  fi
}

install_shell_tools_cargo() {
  local tools=("$@")

  if ! command_exists cargo; then
    log_error "Cargo not available. Cannot install tools via cargo."
    return 1
  fi

  local tmpdir
  tmpdir=$(mktemp -d)
  trap "rm -rf '$tmpdir'" RETURN

  for tool in "${tools[@]}"; do
    case "$tool" in
    yazi | yazi-fm)
      log_info "Installing yazi via cargo..."
      cargo install --locked yazi-fm yazi-cli --root "$tmpdir"
      sudo install -m 755 "$tmpdir/bin/yazi" "${TOOLS_INSTALL_DIR}/yazi"
      sudo install -m 755 "$tmpdir/bin/ya" "${TOOLS_INSTALL_DIR}/ya" 2>/dev/null || true
      ;;
    zoxide)
      log_info "Installing zoxide via cargo..."
      cargo install zoxide --locked --root "$tmpdir"
      sudo install -m 755 "$tmpdir/bin/zoxide" "${TOOLS_INSTALL_DIR}/zoxide"
      ;;
    fd)
      log_info "Installing fd via cargo..."
      cargo install fd-find --root "$tmpdir"
      sudo install -m 755 "$tmpdir/bin/fd" "${TOOLS_INSTALL_DIR}/fd"
      ;;
    ripgrep)
      log_info "Installing ripgrep via cargo..."
      cargo install ripgrep --root "$tmpdir"
      sudo install -m 755 "$tmpdir/bin/rg" "${TOOLS_INSTALL_DIR}/rg"
      ;;
    esac
  done
}

setup_onboarding_templates_repo() {
  local repo_url="${ONBOARDING_TEMPLATES_REPO:-$ONBOARDING_TEMPLATES_REPO_DEFAULT}"
  local target_path="${ONBOARDING_TEMPLATES_PATH:-$ONBOARDING_TEMPLATES_PATH_DEFAULT}"

  log_step "Setting up onboarding templates repo"

  if ! command -v git >/dev/null 2>&1; then
    log_warn "git not available; skipping onboarding templates clone"
    return 0
  fi

  # Use a temporary location for cloning (preserves SSH agent access)
  local temp_clone_dir="${XDG_CACHE_HOME:-$HOME/.cache}/oqto/oqto-templates-clone"
  mkdir -p "$(dirname "$temp_clone_dir")"

  if [[ -d "$temp_clone_dir/.git" ]]; then
    log_info "Updating onboarding templates..."
    git -C "$temp_clone_dir" fetch --all --prune 2>/dev/null || true
    git -C "$temp_clone_dir" reset --hard origin/main 2>/dev/null || true
  else
    log_info "Cloning onboarding templates repo..."
    rm -rf "$temp_clone_dir"
    # Use GIT_TERMINAL_PROMPT=0 to prevent git from prompting for credentials
    if ! GIT_TERMINAL_PROMPT=0 git clone "$repo_url" "$temp_clone_dir" 2>/dev/null; then
      # Fallback to HTTPS if SSH fails
      local https_url="${repo_url/git@github.com:/https://github.com/}"
      https_url="${https_url%.git}"
      if ! GIT_TERMINAL_PROMPT=0 git clone "$https_url" "$temp_clone_dir" 2>/dev/null; then
        log_warn "Templates repo not available (${repo_url}). Skipping."
        return 0
      fi
    fi
  fi

  # Install the repo clone to the target path
  log_info "Installing templates to $target_path..."
  sudo mkdir -p "$(dirname "$target_path")"
  sudo rm -rf "$target_path"
  sudo cp -r "$temp_clone_dir" "$target_path"
  sudo chmod -R a+rX "$target_path" >/dev/null 2>&1 || true

  # The oqto service runs as user 'oqto' and needs to git pull updates.
  if id oqto &>/dev/null; then
    sudo chown -R oqto:oqto "$target_path"
    sudo -u oqto git config --global --add safe.directory "$target_path" 2>/dev/null || true
  fi

  log_success "Onboarding templates installed"
}

update_external_repos() {
  local repos_dir="${EXTERNAL_REPOS_DIR:-$EXTERNAL_REPOS_DIR_DEFAULT}"

  log_step "Updating external repos in $repos_dir"

  if [[ ! -d "$repos_dir" ]]; then
    return 0
  fi

  if ! command -v git >/dev/null 2>&1; then
    log_warn "git not available; skipping external repo updates"
    return 0
  fi

  local repo
  for repo in "$repos_dir"/*; do
    if [[ -d "$repo/.git" ]]; then
      log_info "Updating $(basename "$repo")"
      sudo git -C "$repo" fetch --all --prune >/dev/null 2>&1 || true
      sudo git -C "$repo" reset --hard origin/main >/dev/null 2>&1 || true
      sudo chmod -R a+rX "$repo" >/dev/null 2>&1 || true
    fi
  done
}

setup_feedback_dirs() {
  local public_path="${FEEDBACK_PUBLIC_DROPBOX:-/usr/local/share/oqto/issues}"
  local private_path="${FEEDBACK_PRIVATE_ARCHIVE:-/var/lib/oqto/issue-archive}"

  log_step "Setting up feedback directories"

  sudo mkdir -p "$public_path" "$private_path" >/dev/null 2>&1 || true
  sudo chmod 1777 "$public_path" >/dev/null 2>&1 || true
  sudo chmod 700 "$private_path" >/dev/null 2>&1 || true
}

