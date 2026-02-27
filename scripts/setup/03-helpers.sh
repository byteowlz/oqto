# ==============================================================================
# Helper Functions
# ==============================================================================

log_info() {
  echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
  echo -e "${GREEN}[OK]${NC} $*"
}

log_warn() {
  echo -e "${YELLOW}[WARN]${NC} $*"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $*" >&2
}

log_step() {
  echo -e "\n${BOLD}${CYAN}==>${NC} ${BOLD}$*${NC}"
}

confirm() {
  local prompt="${1:-Continue?}"
  local default="${2:-y}"

  if [[ "$NONINTERACTIVE" == "true" ]]; then
    return 0
  fi

  local yn
  if [[ "$default" == "y" ]]; then
    read -r -p "$prompt [Y/n] " yn
    yn="${yn:-y}"
  else
    read -r -p "$prompt [y/N] " yn
    yn="${yn:-n}"
  fi

  [[ "$yn" =~ ^[Yy] ]]
}

prompt_choice() {
  local prompt="$1"
  shift
  local options=("$@")
  local default="${options[0]}"

  if [[ "$NONINTERACTIVE" == "true" ]]; then
    echo "$default"
    return
  fi

  echo -e "\n${BOLD}$prompt${NC}" >&2
  local i=1
  for opt in "${options[@]}"; do
    if [[ $i -eq 1 ]]; then
      echo "  $i) $opt (default)" >&2
    else
      echo "  $i) $opt" >&2
    fi
    ((i++))
  done

  local choice
  read -r -p "Enter choice [1-${#options[@]}]: " choice
  choice="${choice:-1}"

  if [[ "$choice" =~ ^[0-9]+$ ]] && [[ "$choice" -ge 1 ]] && [[ "$choice" -le "${#options[@]}" ]]; then
    echo "${options[$((choice - 1))]}"
  else
    echo "$default"
  fi
}

prompt_input() {
  local prompt="$1"
  local default="${2:-}"

  if [[ "$NONINTERACTIVE" == "true" ]]; then
    echo "$default"
    return
  fi

  local input
  if [[ -n "$default" ]]; then
    read -r -p "$prompt [$default]: " input
    echo "${input:-$default}"
  else
    read -r -p "$prompt: " input
    echo "$input"
  fi
}

prompt_password() {
  local prompt="$1"

  if [[ "$NONINTERACTIVE" == "true" ]]; then
    # Generate random password in non-interactive mode
    openssl rand -base64 16 | tr -d '/+=' | head -c 16
    return
  fi

  local password
  # Read from /dev/tty explicitly so this works inside $() subshells
  read -r -s -p "$prompt: " password </dev/tty
  echo >&2
  echo "$password"
}

command_exists() {
  command -v "$1" &>/dev/null
}

# Package manager update flags (avoid repeated updates)
APT_UPDATED="false"

apt_update_once() {
  local force="${1:-}"
  if [[ "$APT_UPDATED" != "true" || "$force" == "force" ]]; then
    log_info "Updating apt package index..."
    sudo apt-get update
    APT_UPDATED="true"
  fi
}

# ==============================================================================
# OS Detection
# ==============================================================================

detect_os() {
  case "$(uname -s)" in
  Darwin)
    OS="macos"
    OS_VERSION="$(sw_vers -productVersion)"
    ARCH="$(uname -m)"
    ;;
  Linux)
    OS="linux"
    if [[ -f /etc/os-release ]]; then
      # shellcheck source=/dev/null
      source /etc/os-release
      OS_DISTRO="${ID:-unknown}"
      OS_VERSION="${VERSION_ID:-unknown}"
    else
      OS_DISTRO="unknown"
      OS_VERSION="unknown"
    fi
    ARCH="$(uname -m)"
    ;;
  *)
    log_error "Unsupported operating system: $(uname -s)"
    exit 1
    ;;
  esac

  log_info "Detected: $OS ($ARCH)"
  if [[ "$OS" == "linux" ]]; then
    log_info "Distribution: $OS_DISTRO $OS_VERSION"
  else
    log_info "Version: $OS_VERSION"
  fi
}

# Install or upgrade Node.js to the latest LTS version.
# Uses the official Node.js binary tarball for consistent results across distros.
install_latest_nodejs() {
  local NODE_INSTALL_DIR="/usr/local"
  local DESIRED_MAJOR="22"  # Current LTS line (update when LTS changes)

  # Check if node exists and is already the desired major version
  if command_exists node; then
    local current_version
    current_version="$(node --version 2>/dev/null | sed 's/^v//')"
    local current_major="${current_version%%.*}"
    if [[ "$current_major" == "$DESIRED_MAJOR" ]]; then
      log_success "Node.js: v${current_version} (LTS ${DESIRED_MAJOR}.x)"
      return 0
    fi
    log_info "Node.js v${current_version} installed, upgrading to LTS ${DESIRED_MAJOR}.x..."
  else
    log_info "Installing Node.js LTS ${DESIRED_MAJOR}.x..."
  fi

  # Determine architecture for the download URL
  local node_arch
  case "$(uname -m)" in
    x86_64)  node_arch="x64" ;;
    aarch64) node_arch="arm64" ;;
    armv7l)  node_arch="armv7l" ;;
    *)
      log_warn "Unsupported architecture $(uname -m) for Node.js binary install"
      log_warn "Falling back to distro package..."
      case "$OS_DISTRO" in
        arch | manjaro | endeavouros) sudo pacman -S --noconfirm nodejs ;;
        debian | ubuntu | pop | linuxmint) apt_update_once; sudo apt-get install -y nodejs ;;
        fedora | centos | rhel | rocky | alma*) sudo dnf install -y nodejs ;;
        opensuse*) sudo zypper install -y nodejs ;;
        *) log_warn "Please install Node.js manually" ;;
      esac
      return 0
      ;;
  esac

  # Fetch the latest LTS version number from nodejs.org
  local latest_version
  latest_version="$(curl -fsSL "https://nodejs.org/dist/latest-v${DESIRED_MAJOR}.x/" \
    | grep -oP 'node-v\K[0-9]+\.[0-9]+\.[0-9]+' | head -1)"

  if [[ -z "$latest_version" ]]; then
    log_warn "Could not determine latest Node.js version, falling back to distro package"
    case "$OS_DISTRO" in
      arch | manjaro | endeavouros) sudo pacman -S --noconfirm nodejs ;;
      debian | ubuntu | pop | linuxmint) apt_update_once; sudo apt-get install -y nodejs ;;
      fedora | centos | rhel | rocky | alma*) sudo dnf install -y nodejs ;;
      opensuse*) sudo zypper install -y nodejs ;;
      *) log_warn "Please install Node.js manually" ;;
    esac
    return 0
  fi

  local tarball="node-v${latest_version}-linux-${node_arch}.tar.xz"
  local url="https://nodejs.org/dist/v${latest_version}/${tarball}"
  local tmp_dir
  tmp_dir="$(mktemp -d)"

  log_info "Downloading Node.js v${latest_version} for ${node_arch}..."
  if ! curl -fsSL "$url" -o "${tmp_dir}/${tarball}"; then
    log_warn "Download failed, falling back to distro package"
    rm -rf "$tmp_dir"
    case "$OS_DISTRO" in
      arch | manjaro | endeavouros) sudo pacman -S --noconfirm nodejs ;;
      debian | ubuntu | pop | linuxmint) apt_update_once; sudo apt-get install -y nodejs ;;
      fedora | centos | rhel | rocky | alma*) sudo dnf install -y nodejs ;;
      opensuse*) sudo zypper install -y nodejs ;;
      *) log_warn "Please install Node.js manually" ;;
    esac
    return 0
  fi

  # Extract directly into /usr/local (bin/, lib/, include/, share/)
  sudo tar -xJf "${tmp_dir}/${tarball}" -C "$NODE_INSTALL_DIR" --strip-components=1
  rm -rf "$tmp_dir"

  log_success "Node.js: v${latest_version} installed to ${NODE_INSTALL_DIR}"
}

