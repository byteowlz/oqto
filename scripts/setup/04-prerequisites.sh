# ==============================================================================
# Prerequisite Checks
# ==============================================================================

check_prerequisites() {
  log_step "Checking prerequisites"

  local missing=()

  # Required tools
  if ! command_exists git; then
    missing+=("git")
  fi

  if ! command_exists curl; then
    missing+=("curl")
  fi

  # Build essentials — cc/gcc linker is required by Rust and many native deps
  if ! command_exists cc && ! command_exists gcc; then
    log_info "Installing build tools (C compiler required by Rust linker)..."
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros) sudo pacman -S --noconfirm base-devel ;;
    debian | ubuntu | pop | linuxmint) apt_update_once; sudo apt-get install -y build-essential ;;
    fedora | centos | rhel | rocky | alma*) sudo dnf groupinstall -y "Development Tools" ;;
    opensuse*) sudo zypper install -y -t pattern devel_basis ;;
    *) log_warn "Please install a C compiler (gcc/cc) manually" ;;
    esac
    # Verify installation succeeded
    if ! command_exists cc && ! command_exists gcc; then
      log_error "Failed to install C compiler. Rust crate compilation will fail."
      missing+=("cc (C compiler)")
    fi
  fi

  # Ensure cc exists — some distros only install gcc without a cc symlink
  if command_exists gcc && ! command_exists cc; then
    sudo ln -sf "$(command -v gcc)" /usr/local/bin/cc
    log_info "Created cc -> gcc symlink"
  fi

  # unzip is required by the Bun installer — install before Bun check
  if ! command_exists unzip; then
    log_info "Installing unzip (required by Bun installer)..."
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros) sudo pacman -S --noconfirm unzip ;;
    debian | ubuntu | pop | linuxmint) apt_update_once; sudo apt-get install -y unzip ;;
    fedora | centos | rhel | rocky | alma*) sudo dnf install -y unzip ;;
    opensuse*) sudo zypper install -y unzip ;;
    *) log_warn "Please install unzip manually" ;;
    esac
  fi

  # protoc is required by hstry-core (prost-build) for gRPC protobuf compilation
  if ! command_exists protoc; then
    log_info "Installing protobuf compiler (required by hstry gRPC build)..."
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros) sudo pacman -S --noconfirm protobuf ;;
    debian | ubuntu | pop | linuxmint) apt_update_once; sudo apt-get install -y protobuf-compiler ;;
    fedora | centos | rhel | rocky | alma*) sudo dnf install -y protobuf-compiler ;;
    opensuse*) sudo zypper install -y protobuf-devel ;;
    *) log_warn "Please install protoc manually: https://github.com/protocolbuffers/protobuf/releases" ;;
    esac
  fi

  # Rust toolchain
  # Source cargo env if cargo exists but isn't in PATH (e.g. pre-existing install)
  if ! command_exists cargo && [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
  fi
  if ! command_exists cargo; then
    log_warn "Rust toolchain not found"
    if confirm "Install Rust via rustup?"; then
      install_rust
    else
      missing+=("cargo (Rust toolchain)")
    fi
  else
    log_success "Rust: $(cargo --version)"
  fi

  # Bun (for frontend and pi)
  # Source bun env if bun exists but isn't in PATH
  if ! command_exists bun && [[ -d "$HOME/.bun/bin" ]]; then
    export BUN_INSTALL="$HOME/.bun"
    export PATH="$BUN_INSTALL/bin:$PATH"
  fi
  if ! command_exists bun || ! bun --version >/dev/null 2>&1; then
    log_warn "Bun not found or broken"
    if confirm "Install Bun?"; then
      install_bun
    else
      missing+=("bun")
    fi
  else
    log_success "Bun: $(bun --version)"
    # Always ensure bun and pi are globally accessible
    ensure_bun_and_pi_global
  fi

  # Node.js -- needed by some global npm packages (slidev shebang, etc.)
  if ! command_exists node; then
    log_info "Installing Node.js..."
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros) sudo pacman -S --noconfirm nodejs ;;
    debian | ubuntu | pop | linuxmint) apt_update_once; sudo apt-get install -y nodejs ;;
    fedora | centos | rhel | rocky | alma*) sudo dnf install -y nodejs ;;
    opensuse*) sudo zypper install -y nodejs ;;
    *)
      if command_exists bun; then
        log_info "No package manager match, skipping node (bun available)"
      else
        log_warn "Please install Node.js manually"
      fi
      ;;
    esac
  fi

  # Check container runtime if container mode selected
  local backend_mode="${SELECTED_BACKEND_MODE:-$OQTO_BACKEND_MODE}"
  if [[ "$backend_mode" == "container" ]]; then
    check_container_runtime
  fi

  if [[ ${#missing[@]} -gt 0 ]]; then
    log_error "Missing required tools: ${missing[*]}"
    log_error "Please install them and run this script again."
    exit 1
  fi

  log_success "All prerequisites satisfied"

  # Install system prerequisites that may be missing
  install_system_prerequisites
}

install_system_prerequisites() {
  log_info "Checking system prerequisites..."

  local pkgs=()

  # bubblewrap is required for Pi process sandboxing
  if ! command_exists bwrap; then
    pkgs+=("bubblewrap")
  fi

  # sqlite3 CLI is useful for debugging
  if ! command_exists sqlite3; then
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros) pkgs+=("sqlite") ;;
    *) pkgs+=("sqlite3") ;;
    esac
  fi

  # pkg-config and OpenSSL headers are needed for Rust tools (e.g., ignr)
  if [[ "$OS" == "linux" ]]; then
    local need_pkg_config="false"
    if ! command_exists pkg-config; then
      need_pkg_config="true"
    elif ! pkg-config --exists openssl 2>/dev/null; then
      need_pkg_config="true"
    fi

    if [[ "$need_pkg_config" == "true" ]]; then
      case "$OS_DISTRO" in
      arch | manjaro | endeavouros) pkgs+=("pkgconf") ;;
      debian | ubuntu | pop | linuxmint) pkgs+=("pkg-config") ;;
      fedora | centos | rhel | rocky | alma*) pkgs+=("pkgconf-pkg-config") ;;
      opensuse*) pkgs+=("pkg-config") ;;
      esac
    fi

    if [[ ! -f /usr/include/openssl/ssl.h ]] || ! pkg-config --exists openssl 2>/dev/null; then
      case "$OS_DISTRO" in
      arch | manjaro | endeavouros) pkgs+=("openssl") ;;
      debian | ubuntu | pop | linuxmint) pkgs+=("libssl-dev") ;;
      fedora | centos | rhel | rocky | alma*) pkgs+=("openssl-devel") ;;
      opensuse*) pkgs+=("libopenssl-devel") ;;
      esac
    fi
  elif [[ "$OS" == "macos" ]]; then
    if command_exists brew; then
      if ! command_exists pkg-config; then
        brew install pkg-config
      fi
      if ! command_exists openssl; then
        brew install openssl@3
      fi
    fi
  fi

  # zsh is the default shell for platform users
  if ! command_exists zsh; then
    pkgs+=("zsh")
  fi

  # ffmpeg is needed for audio/video processing (voice mode, media previews)
  if ! command_exists ffmpeg; then
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros) pkgs+=("ffmpeg") ;;
    *) pkgs+=("ffmpeg") ;;
    esac
  fi

  # ImageMagick is needed for image processing (thumbnails, conversions)
  if ! command_exists convert && ! command_exists magick; then
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros) pkgs+=("imagemagick") ;;
    debian | ubuntu | pop | linuxmint) pkgs+=("imagemagick") ;;
    fedora | centos | rhel | rocky | alma*) pkgs+=("ImageMagick") ;;
    opensuse*) pkgs+=("ImageMagick") ;;
    esac
  fi

  # starship prompt for a nice terminal experience
  if ! command_exists starship; then
    log_info "Installing starship prompt..."
    if curl -sS https://starship.rs/install.sh | sh -s -- -y >/dev/null 2>&1; then
      log_success "starship installed"
    else
      log_warn "Failed to install starship. Users will have a basic prompt."
    fi
  fi

  if [[ ${#pkgs[@]} -eq 0 ]]; then
    log_success "All system prerequisites already installed"
    return 0
  fi

  log_info "Installing system prerequisites: ${pkgs[*]}"
  case "$OS_DISTRO" in
  arch | manjaro | endeavouros)
    sudo pacman -S --noconfirm "${pkgs[@]}"
    ;;
  debian | ubuntu | pop | linuxmint)
    apt_update_once
    sudo apt-get install -y "${pkgs[@]}"
    ;;
  fedora | centos | rhel | rocky | alma*)
    sudo dnf install -y "${pkgs[@]}"
    ;;
  opensuse*)
    sudo zypper install -y "${pkgs[@]}"
    ;;
  *)
    log_warn "Unknown distribution $OS_DISTRO. Please install manually: ${pkgs[*]}"
    ;;
  esac

  if command_exists bwrap; then
    log_success "bubblewrap (bwrap) installed"
  else
    log_warn "bubblewrap (bwrap) not installed. Pi sandboxing will be disabled."
  fi

  if command_exists zsh; then
    log_success "zsh installed"
  else
    log_warn "zsh not installed. Platform users will fall back to bash."
  fi

  if command_exists starship; then
    log_success "starship prompt installed"
  else
    log_warn "starship not installed. Platform users will have a basic prompt."
  fi
}

check_container_runtime() {
  if [[ "$OQTO_CONTAINER_RUNTIME" == "auto" ]]; then
    if command_exists docker; then
      CONTAINER_RUNTIME="docker"
    elif command_exists podman; then
      CONTAINER_RUNTIME="podman"
    else
      log_warn "No container runtime found (docker or podman)"
      if [[ "$OS" == "macos" ]]; then
        log_info "For macOS multi-user mode, Docker Desktop is recommended"
        if confirm "Install Docker Desktop? (opens download page)"; then
          open "https://www.docker.com/products/docker-desktop/"
          log_info "Please install Docker Desktop and run this script again"
          exit 0
        fi
      fi
      return 1
    fi
  else
    CONTAINER_RUNTIME="$OQTO_CONTAINER_RUNTIME"
    if ! command_exists "$CONTAINER_RUNTIME"; then
      log_error "Specified container runtime not found: $CONTAINER_RUNTIME"
      return 1
    fi
  fi

  log_success "Container runtime: $CONTAINER_RUNTIME ($($CONTAINER_RUNTIME --version))"
}

