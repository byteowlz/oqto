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

