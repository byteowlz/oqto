#!/usr/bin/env bash
#
# Octo Setup Script
# Comprehensive setup and onboarding for the Octo AI Agent Workspace Platform
#
# Supports:
#   - macOS and Linux
#   - Single-user and multi-user modes
#   - Local (native processes) and container (Docker/Podman) modes
#
# Usage:
#   ./setup.sh                  # Interactive mode
#   ./setup.sh --non-interactive # Use defaults or environment variables
#   ./setup.sh --help           # Show help

set -euo pipefail

# ==============================================================================
# Configuration and Defaults
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEMPLATES_DIR="${SCRIPT_DIR}/templates"
ONBOARDING_TEMPLATES_REPO_DEFAULT="https://github.com/byteowlz/octo-templates"
EXTERNAL_REPOS_DIR_DEFAULT="/usr/local/share/octo/external-repos"
ONBOARDING_TEMPLATES_PATH_DEFAULT="/usr/share/octo/octo-templates/dotfiles_users/"
PROJECT_TEMPLATES_PATH_DEFAULT="/usr/share/octo/octo-templates/agents/"

# Default values (can be overridden by environment variables)
: "${OCTO_USER_MODE:=single}"        # single or multi
: "${OCTO_BACKEND_MODE:=local}"      # local or container
: "${OCTO_CONTAINER_RUNTIME:=auto}"  # docker, podman, or auto
: "${OCTO_INSTALL_DEPS:=yes}"        # yes or no
: "${OCTO_INSTALL_SERVICE:=yes}"     # yes or no
: "${OCTO_INSTALL_AGENT_TOOLS:=yes}" # yes or no (agntz, mmry, trx)
: "${OCTO_DEV_MODE:=}"               # true or false (auth dev mode) - empty = prompt
: "${OCTO_LOG_LEVEL:=info}"          # error, warn, info, debug, trace
: "${OCTO_SETUP_CADDY:=}"            # yes or no - empty = prompt
: "${OCTO_DOMAIN:=}"                 # domain for HTTPS (e.g., octo.example.com)

# Server hardening options (Linux only, requires root)
: "${OCTO_HARDEN_SERVER:=}"         # yes or no - empty = prompt in production mode
: "${OCTO_SSH_PORT:=22}"            # SSH port (change if needed)
: "${OCTO_SETUP_FIREWALL:=yes}"     # Configure UFW/firewalld
: "${OCTO_SETUP_FAIL2BAN:=yes}"     # Install and configure fail2ban
: "${OCTO_HARDEN_SSH:=yes}"         # Apply SSH hardening config
: "${OCTO_SETUP_AUTO_UPDATES:=yes}" # Enable automatic security updates
: "${OCTO_HARDEN_KERNEL:=yes}"      # Apply kernel security parameters

# Agent tools installation tracking
INSTALL_MMRY="false"
INSTALL_ALL_TOOLS="false"

# LLM provider configuration (set during generate_config)
LLM_PROVIDER=""
LLM_API_KEY_SET="false"
EAVS_ENABLED="false"

# Production configuration (set during setup)
PRODUCTION_MODE="false"
SETUP_CADDY="false"
DOMAIN=""
JWT_SECRET=""
ADMIN_USERNAME=""
ADMIN_PASSWORD=""
ADMIN_EMAIL=""

# Paths (XDG compliant)
: "${XDG_CONFIG_HOME:=$HOME/.config}"
: "${XDG_DATA_HOME:=$HOME/.local/share}"
: "${XDG_STATE_HOME:=$HOME/.local/state}"

OCTO_CONFIG_DIR="${XDG_CONFIG_HOME}/octo"
OCTO_DATA_DIR="${XDG_DATA_HOME}/octo"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

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
  read -r -s -p "$prompt: " password
  echo
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

  # Rust toolchain
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

  # Bun (for frontend)
  if ! command_exists bun; then
    log_warn "Bun not found"
    if confirm "Install Bun?"; then
      install_bun
    else
      missing+=("bun")
    fi
  else
    log_success "Bun: $(bun --version)"
  fi

  # Check container runtime if container mode selected
  if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
    check_container_runtime
  fi

  if [[ ${#missing[@]} -gt 0 ]]; then
    log_error "Missing required tools: ${missing[*]}"
    log_error "Please install them and run this script again."
    exit 1
  fi

  log_success "All prerequisites satisfied"
}

check_container_runtime() {
  if [[ "$OCTO_CONTAINER_RUNTIME" == "auto" ]]; then
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
    CONTAINER_RUNTIME="$OCTO_CONTAINER_RUNTIME"
    if ! command_exists "$CONTAINER_RUNTIME"; then
      log_error "Specified container runtime not found: $CONTAINER_RUNTIME"
      return 1
    fi
  fi

  log_success "Container runtime: $CONTAINER_RUNTIME ($($CONTAINER_RUNTIME --version))"
}

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

# ==============================================================================
# Pi Extensions Installation
# ==============================================================================

# GitHub repo for Pi agent extensions
PI_EXTENSIONS_REPO="https://github.com/byteowlz/pi-agent-extensions.git"

# Default extensions to install (subset of what's available in the repo)
# These are the Octo-relevant extensions; users can install others via the
# pi-agent-extensions justfile.
PI_DEFAULT_EXTENSIONS=(
  "auto-rename"
  "octo-bridge"
  "octo-todos"
  "custom-context-files"
)

# Clone or update the pi-agent-extensions repo into a cache directory
clone_pi_extensions_repo() {
  local cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/octo/pi-agent-extensions"

  if [[ -d "$cache_dir/.git" ]]; then
    log_info "Updating pi-agent-extensions repo..."
    git -C "$cache_dir" fetch --all --prune 2>/dev/null || true
    git -C "$cache_dir" reset --hard origin/main 2>/dev/null || true
  else
    # Remove stale cache dir if it exists without .git
    if [[ -d "$cache_dir" ]]; then
      rm -rf "$cache_dir"
    fi
    log_info "Cloning pi-agent-extensions from GitHub..."
    mkdir -p "$(dirname "$cache_dir")"
    if ! git clone --depth 1 "$PI_EXTENSIONS_REPO" "$cache_dir"; then
      log_error "Failed to clone pi-agent-extensions repo"
      return 1
    fi
  fi

  # Verify the clone has content
  if [[ ! -f "$cache_dir/README.md" ]]; then
    log_error "pi-agent-extensions clone appears empty at $cache_dir"
    rm -rf "$cache_dir"
    return 1
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

    # Remove files that should not be in the install target
    rm -f "$dest_dir/package.json" "$dest_dir/install.sh"

    log_success "Installed $ext_name extension"
    ((installed++)) || true
  done

  # Create a README for the user
  cat >"${extensions_dir}/README.md" <<'EOF'
# Pi Agent Extensions (installed by Octo)

These extensions are installed by Octo setup from the pi-agent-extensions
repository: https://github.com/byteowlz/pi-agent-extensions

## Installed Extensions

- **auto-rename**: Automatically generate session names from first user query
- **octo-bridge**: Emit granular agent phase status for the Octo runner
- **octo-todos**: Todo management tools for Octo frontend integration
- **custom-context-files**: Auto-load USER.md, PERSONALITY.md, and other context files into prompts

## Managing Extensions

To install additional extensions or update existing ones, clone the
pi-agent-extensions repo and use its justfile:

    git clone https://github.com/byteowlz/pi-agent-extensions.git
    cd pi-agent-extensions
    just install      # Interactive picker
    just install-all  # Install everything
    just status       # Show sync status

Or re-run the Octo setup script to update the default set.
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

# ==============================================================================
# Shell Tools Installation
# ==============================================================================

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
    esac
  done

  if [[ ${#apt_pkgs[@]} -gt 0 ]]; then
    log_info "Installing via apt: ${apt_pkgs[*]}"
    apt_update_once
    sudo apt-get install -y "${apt_pkgs[@]}"
  fi

  if [[ ${#cargo_pkgs[@]} -gt 0 ]]; then
    install_shell_tools_cargo "${cargo_pkgs[@]}"
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
    esac
  done

  if [[ ${#dnf_pkgs[@]} -gt 0 ]]; then
    log_info "Installing via dnf: ${dnf_pkgs[*]}"
    sudo dnf install -y "${dnf_pkgs[@]}"
  fi

  if [[ ${#cargo_pkgs[@]} -gt 0 ]]; then
    install_shell_tools_cargo "${cargo_pkgs[@]}"
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
    esac
  done

  if [[ ${#zypper_pkgs[@]} -gt 0 ]]; then
    log_info "Installing via zypper: ${zypper_pkgs[*]}"
    sudo zypper install -y "${zypper_pkgs[@]}"
  fi

  if [[ ${#cargo_pkgs[@]} -gt 0 ]]; then
    install_shell_tools_cargo "${cargo_pkgs[@]}"
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

  if command -v git >/dev/null 2>&1; then
    if [[ -d "$target_path/.git" ]]; then
      log_info "Updating onboarding templates in $target_path"
      sudo git -C "$target_path" fetch --all --prune >/dev/null 2>&1 || true
      sudo git -C "$target_path" reset --hard origin/main >/dev/null 2>&1 || true
    else
      log_info "Cloning onboarding templates repo to $target_path"
      sudo mkdir -p "$(dirname "$target_path")"
      sudo git clone "$repo_url" "$target_path" >/dev/null 2>&1 || true
    fi
    sudo chmod -R a+rX "$target_path" >/dev/null 2>&1 || true
  else
    log_warn "git not available; skipping onboarding templates clone"
  fi
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
  local public_path="${FEEDBACK_PUBLIC_DROPBOX:-/usr/local/share/octo/issues}"
  local private_path="${FEEDBACK_PRIVATE_ARCHIVE:-/var/lib/octo/issue-archive}"

  log_step "Setting up feedback directories"

  sudo mkdir -p "$public_path" "$private_path" >/dev/null 2>&1 || true
  sudo chmod 1777 "$public_path" >/dev/null 2>&1 || true
  sudo chmod 700 "$private_path" >/dev/null 2>&1 || true
}

# ==============================================================================
# ==============================================================================
# Agent Tools Installation
# ==============================================================================
#
# Tools for AI agents in the Octo platform:
#
#   agntz   - Agent toolkit (wraps other tools, file reservations, etc.)
#   mmry    - Memory storage and semantic search
#   trx     - Issue/task tracking
#   scrpr   - Web content extraction (readability, Tavily, Jina)
#   tmpltr  - Document generation from templates (Typst)
#   sldr    - Markdown presentations (Slidev)
#   ignr    - Gitignore generation (auto-detect languages/tools)
#
# Installation sources (in order of preference):
#   1. cargo install / go install from registries
#   2. cargo install --git / go install from GitHub
#   3. Local build from source (if available)
#
# ==============================================================================

# GitHub org for byteowlz tools
BYTEOWLZ_GITHUB="https://github.com/byteowlz"

# Global install directory for all agent tools
TOOLS_INSTALL_DIR="/usr/local/bin"

# Move a built binary into TOOLS_INSTALL_DIR
install_binary_global() {
  local binary_path="$1"
  local tool="$2"

  if [[ ! -f "$binary_path" ]]; then
    log_warn "Binary not found: $binary_path"
    return 1
  fi

  sudo install -m 755 "$binary_path" "${TOOLS_INSTALL_DIR}/${tool}"
  log_success "$tool installed to ${TOOLS_INSTALL_DIR}/${tool}"
}

# Install a Rust tool from crates.io, GitHub, or local source.
# Installs to /usr/local/bin so all users can access it.
install_rust_tool() {
  local tool="$1"
  local repo="${2:-$tool}" # repo name, defaults to tool name

  if command_exists "$tool"; then
    local version
    version=$("$tool" --version 2>/dev/null | head -1 || echo 'unknown')
    log_success "$tool already installed: $version"
    return 0
  fi

  if ! command_exists cargo; then
    log_error "Cargo not available. Cannot install $tool."
    return 1
  fi

  log_info "Installing $tool..."

  local tmpdir
  tmpdir=$(mktemp -d)
  trap "rm -rf '$tmpdir'" RETURN

  # Try crates.io first
  if cargo install "$tool" --root "$tmpdir" 2>/dev/null; then
    install_binary_global "$tmpdir/bin/$tool" "$tool"
    return 0
  fi

  # Try GitHub
  log_info "Trying GitHub repository..."
  if cargo install --git "${BYTEOWLZ_GITHUB}/${repo}.git" --root "$tmpdir" 2>/dev/null; then
    install_binary_global "$tmpdir/bin/$tool" "$tool"
    return 0
  fi

  # Check for local source directory
  local local_path=""
  for base in "$HOME/byteowlz" "$HOME/code/byteowlz" "/opt/byteowlz"; do
    if [[ -d "$base/$repo" ]]; then
      local_path="$base/$repo"
      break
    fi
  done

  if [[ -n "$local_path" && -f "$local_path/Cargo.toml" ]]; then
    log_info "Installing from local path: $local_path"
    if cargo install --path "$local_path" --root "$tmpdir" 2>/dev/null; then
      install_binary_global "$tmpdir/bin/$tool" "$tool"
      return 0
    fi
  fi

  log_warn "Failed to install $tool"
  return 1
}

# Install a Go tool from GitHub or local source.
# Installs to /usr/local/bin so all users can access it.
# Handles both root-level main.go and cmd/<tool>/main.go layouts.
install_go_tool() {
  local tool="$1"
  local repo="${2:-$tool}" # repo name, defaults to tool name
  local go_module="github.com/byteowlz/${repo}"

  if command_exists "$tool"; then
    local version
    version=$("$tool" --version 2>/dev/null | head -1 || echo 'unknown')
    log_success "$tool already installed: $version"
    return 0
  fi

  if ! command_exists go; then
    log_warn "Go not available. Cannot install $tool."
    return 1
  fi

  log_info "Installing $tool..."

  local tmpdir
  tmpdir=$(mktemp -d)
  trap "rm -rf '$tmpdir'" RETURN

  # Try go install from GitHub (root package first, then cmd/<tool>)
  if GOBIN="$tmpdir" go install "${go_module}@latest" 2>/dev/null; then
    install_binary_global "$tmpdir/$tool" "$tool"
    return 0
  fi

  if GOBIN="$tmpdir" go install "${go_module}/cmd/${tool}@latest" 2>/dev/null; then
    install_binary_global "$tmpdir/$tool" "$tool"
    return 0
  fi

  # Check for local source directory
  local local_path=""
  for base in "$HOME/byteowlz" "$HOME/code/byteowlz" "/opt/byteowlz"; do
    if [[ -d "$base/$repo" ]]; then
      local_path="$base/$repo"
      break
    fi
  done

  if [[ -n "$local_path" && -f "$local_path/go.mod" ]]; then
    log_info "Installing from local path: $local_path"
    if [[ -f "$local_path/cmd/$tool/main.go" ]]; then
      if (cd "$local_path" && GOBIN="$tmpdir" go install "./cmd/$tool" 2>/dev/null); then
        install_binary_global "$tmpdir/$tool" "$tool"
        return 0
      fi
    elif (cd "$local_path" && GOBIN="$tmpdir" go install . 2>/dev/null); then
      install_binary_global "$tmpdir/$tool" "$tool"
      return 0
    fi
  fi

  # Fallback: clone repo and build locally (handles mismatched module paths)
  log_info "Trying clone and build..."
  local clone_dir="${tmpdir}/src"
  if git clone --depth 1 "${BYTEOWLZ_GITHUB}/${repo}.git" "$clone_dir" 2>/dev/null; then
    if [[ -f "$clone_dir/cmd/$tool/main.go" ]]; then
      if (cd "$clone_dir" && GOBIN="$tmpdir" go install "./cmd/$tool"); then
        install_binary_global "$tmpdir/$tool" "$tool"
        return 0
      fi
    elif (cd "$clone_dir" && GOBIN="$tmpdir" go install .); then
      install_binary_global "$tmpdir/$tool" "$tool"
      return 0
    fi
  fi

  log_warn "Failed to install $tool"
  return 1
}

install_agntz() {
  log_step "Installing agntz (Agent Toolkit)"
  install_rust_tool agntz
}

install_all_agent_tools() {
  log_step "Installing agent tools"

  # Core tools (Rust)
  install_rust_tool agntz
  install_rust_tool mmry
  install_rust_tool tmpltr
  install_rust_tool sldr
  install_rust_tool ignr

  # Core tools (Go)
  install_go_tool scrpr
  install_go_tool sx
}

select_agent_tools() {
  log_step "Agent Tools Selection"

  echo
  echo "Octo can install agent tools:"
  echo
  echo -e "  ${BOLD}Core tools (recommended):${NC}"
  echo "    agntz   - Agent toolkit (file reservations, tool management)"
  echo "    mmry    - Memory storage and semantic search"
  echo "    scrpr   - Web content extraction"
  echo "    sx      - Web search via local SearXNG instance"
  echo
  echo -e "  ${BOLD}Additional tools:${NC}"
  echo "    tmpltr  - Document generation from templates"
  echo "    sldr    - Markdown presentations"
  echo "    ignr    - Gitignore generation"
  echo "    trx     - Issue/task tracking"
  echo
  echo "  Installing sx will also set up a local SearXNG search engine"
  echo "  with Valkey for caching (binds to 127.0.0.1:8888)."
  echo

  if confirm "Install all agent tools (recommended)?"; then
    INSTALL_MMRY="true"
    INSTALL_ALL_TOOLS="true"
  else
    if confirm "Install mmry (memory system)?"; then
      INSTALL_MMRY="true"
    fi
  fi
}

install_agent_tools_selected() {
  log_step "Installing agent tools"

  if [[ "$INSTALL_ALL_TOOLS" == "true" ]]; then
    install_all_agent_tools
    return
  fi

  if [[ "$INSTALL_MMRY" == "true" ]]; then
    install_rust_tool mmry
  fi

  # Use agntz tools install for additional tools if agntz is available
  if command_exists agntz; then
    log_info "Running agntz doctor to check tool health..."
    agntz tools doctor 2>/dev/null || true
  fi
}

# ==============================================================================
# SearXNG Installation (local search engine for agents)
# ==============================================================================
#
# SearXNG is a privacy-respecting metasearch engine. We install it locally so
# agents can use `sx` for web searches without depending on external APIs.
#
# Architecture:
#   - Runs under the backend user (current user in single-user, 'octo' in multi-user)
#   - Binds to 127.0.0.1:8888 (local only, no external exposure)
#   - JSON API enabled so `sx` can query it programmatically
#   - Valkey (Redis-compatible) for rate limiting and caching
#   - Managed via systemd user service (single-user) or system service (multi-user)
#
# ==============================================================================

: "${SEARXNG_PORT:=8888}"
: "${SEARXNG_BIND:=127.0.0.1}"

install_searxng() {
  log_step "Installing SearXNG (local search engine)"

  # Determine install paths based on user mode
  local searxng_base searxng_user service_type
  if [[ "$SELECTED_USER_MODE" == "multi" ]]; then
    searxng_user="octo"
    searxng_base="${OCTO_HOME}/.local/share/searxng"
    service_type="system"
  else
    searxng_user="$(whoami)"
    searxng_base="${XDG_DATA_HOME}/searxng"
    service_type="user"
  fi

  local searxng_src="${searxng_base}/searxng-src"
  local searxng_venv="${searxng_base}/venv"
  local searxng_settings="${searxng_base}/settings.yml"

  # 1. Install system dependencies
  install_searxng_deps

  # 2. Install Valkey (Redis-compatible key-value store)
  install_valkey

  # 3. Clone SearXNG source
  log_info "Setting up SearXNG in ${searxng_base}..."
  if [[ "$service_type" == "system" ]]; then
    sudo mkdir -p "$searxng_base"
    sudo chown "$searxng_user:$searxng_user" "$searxng_base"
  else
    mkdir -p "$searxng_base"
  fi

  if [[ -d "$searxng_src/.git" ]]; then
    log_info "Updating SearXNG source..."
    run_as_searxng_user "$service_type" "$searxng_user" \
      "git -C '$searxng_src' pull --ff-only" || true
  else
    log_info "Cloning SearXNG..."
    run_as_searxng_user "$service_type" "$searxng_user" \
      "git clone --depth 1 'https://github.com/searxng/searxng' '$searxng_src'"
  fi

  # 4. Create virtualenv and install dependencies
  if [[ ! -d "$searxng_venv" ]]; then
    log_info "Creating Python virtualenv..."
    run_as_searxng_user "$service_type" "$searxng_user" \
      "python3 -m venv '$searxng_venv'"
  fi

  log_info "Installing SearXNG Python dependencies..."
  run_as_searxng_user "$service_type" "$searxng_user" \
    "'$searxng_venv/bin/pip' install -U pip setuptools wheel"
  run_as_searxng_user "$service_type" "$searxng_user" \
    "'$searxng_venv/bin/pip' install -r '$searxng_src/requirements.txt'"
  run_as_searxng_user "$service_type" "$searxng_user" \
    "cd '$searxng_src' && '$searxng_venv/bin/pip' install --use-pep517 --no-build-isolation -e ."

  # 5. Generate settings.yml with JSON API enabled
  generate_searxng_settings "$searxng_settings" "$service_type" "$searxng_user"

  # 6. Install systemd service
  install_searxng_service "$searxng_base" "$searxng_venv" "$searxng_settings" "$service_type" "$searxng_user"

  # 7. Configure sx to use local instance
  configure_sx_for_searxng

  log_success "SearXNG installed and configured"
  log_info "SearXNG API: http://${SEARXNG_BIND}:${SEARXNG_PORT}"
  log_info "Test with: sx 'hello world'"
}

# Helper: run a command as the SearXNG user
run_as_searxng_user() {
  local service_type="$1"
  local user="$2"
  local cmd="$3"

  if [[ "$service_type" == "system" ]]; then
    sudo -H -u "$user" bash -c "$cmd"
  else
    bash -c "$cmd"
  fi
}

install_searxng_deps() {
  log_info "Installing SearXNG system dependencies..."

  case "$OS" in
  linux)
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros)
      sudo pacman -S --noconfirm --needed \
        python python-pip python-virtualenv \
        git base-devel libxslt zlib libffi openssl
      ;;
    debian | ubuntu | pop | linuxmint)
      apt_update_once
      sudo apt-get install -y \
        python3-dev python3-babel python3-venv python-is-python3 \
        git build-essential libxslt-dev zlib1g-dev libffi-dev libssl-dev
      ;;
    fedora | centos | rhel | rocky | alma)
      sudo dnf install -y \
        python3-devel python3-babel python3-virtualenv \
        git gcc libxslt-devel zlib-devel libffi-devel openssl-devel
      ;;
    opensuse* | suse*)
      sudo zypper install -y \
        python3-devel python3-Babel python3-virtualenv \
        git gcc libxslt-devel zlib-devel libffi-devel libopenssl-devel
      ;;
    *)
      log_warn "Unknown distribution. Please install Python 3 dev packages manually."
      ;;
    esac
    ;;
  macos)
    if command_exists brew; then
      brew install python3 libxslt
    else
      log_warn "Homebrew not found. Please install Python 3 manually."
    fi
    ;;
  esac
}

install_valkey() {
  if command_exists valkey-server; then
    log_success "Valkey already installed: $(valkey-server --version 2>/dev/null | head -1)"
  elif command_exists redis-server; then
    log_success "Redis already installed (compatible with Valkey): $(redis-server --version 2>/dev/null | head -1)"
  else
    log_info "Installing Valkey (Redis-compatible key-value store)..."

    case "$OS" in
    linux)
      case "$OS_DISTRO" in
      arch | manjaro | endeavouros)
        sudo pacman -S --noconfirm valkey
        ;;
      debian | ubuntu | pop | linuxmint)
        apt_update_once
        # Valkey may not be in default repos, fall back to redis
        if sudo apt-get install -y valkey 2>/dev/null; then
          true
        else
          log_info "Valkey not in repos, installing Redis instead..."
          sudo apt-get install -y redis-server
        fi
        ;;
      fedora | centos | rhel | rocky | alma)
        sudo dnf install -y valkey 2>/dev/null || sudo dnf install -y redis
        ;;
      opensuse* | suse*)
        sudo zypper install -y valkey 2>/dev/null || sudo zypper install -y redis
        ;;
      *)
        log_warn "Please install Valkey or Redis manually."
        ;;
      esac
      ;;
    macos)
      if command_exists brew; then
        brew install valkey 2>/dev/null || brew install redis
      fi
      ;;
    esac
  fi

  # Enable and start Valkey/Redis
  if command_exists valkey-server; then
    if [[ "$OS" == "linux" ]]; then
      sudo systemctl enable --now valkey 2>/dev/null || sudo systemctl enable --now valkey-server 2>/dev/null || true
    fi
    log_success "Valkey is running"
  elif command_exists redis-server; then
    if [[ "$OS" == "linux" ]]; then
      sudo systemctl enable --now redis 2>/dev/null || sudo systemctl enable --now redis-server 2>/dev/null || true
    fi
    log_success "Redis is running"
  fi
}

generate_searxng_settings() {
  local settings_file="$1"
  local service_type="$2"
  local user="$3"

  log_info "Generating SearXNG settings with JSON API enabled..."

  # Generate a random secret key
  local secret_key
  secret_key=$(generate_secure_secret 32)

  # Detect Valkey/Redis URL
  local kv_url="false"
  if command_exists valkey-server; then
    kv_url="valkey://localhost:6379/0"
  elif command_exists redis-server; then
    kv_url="redis://localhost:6379/0"
  fi

  local settings_content
  read -r -d '' settings_content <<EOSETTINGS || true
# SearXNG settings - generated by Octo setup.sh
# Local instance for agent web search via sx CLI

use_default_settings: true

general:
  debug: false
  instance_name: "Octo SearXNG"

search:
  safe_search: 0
  autocomplete: "duckduckgo"
  # Enable JSON format for sx API access
  formats:
    - html
    - json

server:
  port: ${SEARXNG_PORT}
  bind_address: "${SEARXNG_BIND}"
  secret_key: "${secret_key}"
  limiter: false
  public_instance: false
  image_proxy: false
  http_protocol_version: "1.0"
  method: "GET"

valkey:
  url: ${kv_url}

ui:
  static_use_hash: true

outgoing:
  request_timeout: 10.0
  max_request_timeout: 15.0
  # Use multiple user agents to avoid blocks
  useragent_suffix: ""

engines:
  - name: duckduckgo
    disabled: false
  - name: google
    disabled: false
  - name: brave
    disabled: false
  - name: wikipedia
    disabled: false
  - name: github
    disabled: false
  - name: stackoverflow
    disabled: false
  - name: arch linux wiki
    disabled: false
  - name: npm
    disabled: false
  - name: crates.io
    disabled: false
  - name: pypi
    disabled: false
EOSETTINGS

  if [[ "$service_type" == "system" ]]; then
    echo "$settings_content" | sudo -u "$user" tee "$settings_file" >/dev/null
  else
    echo "$settings_content" >"$settings_file"
  fi

  log_success "SearXNG settings written to $settings_file"
}

install_searxng_service() {
  local searxng_base="$1"
  local searxng_venv="$2"
  local searxng_settings="$3"
  local service_type="$4"
  local user="$5"

  local searxng_src="${searxng_base}/searxng-src"

  if [[ "$service_type" == "user" ]]; then
    # User-level systemd service
    local service_dir="$HOME/.config/systemd/user"
    mkdir -p "$service_dir"

    cat >"${service_dir}/searxng.service" <<EOF
[Unit]
Description=SearXNG local search engine
After=default.target

[Service]
Type=simple
Environment=SEARXNG_SETTINGS_PATH=${searxng_settings}
WorkingDirectory=${searxng_src}
ExecStart=${searxng_venv}/bin/python -m searx.webapp
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    systemctl --user daemon-reload
    systemctl --user enable searxng

    if confirm "Start SearXNG now?"; then
      systemctl --user start searxng
      log_success "SearXNG started (user service)"
      log_info "Check status: systemctl --user status searxng"
    fi

  else
    # System-level systemd service
    sudo tee /etc/systemd/system/searxng.service >/dev/null <<EOF
[Unit]
Description=SearXNG local search engine
After=network.target valkey.service redis.service

[Service]
Type=simple
User=${user}
Group=${user}
Environment=SEARXNG_SETTINGS_PATH=${searxng_settings}
WorkingDirectory=${searxng_src}
ExecStart=${searxng_venv}/bin/python -m searx.webapp
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

    sudo systemctl daemon-reload
    sudo systemctl enable searxng

    if confirm "Start SearXNG now?"; then
      sudo systemctl start searxng
      log_success "SearXNG started (system service)"
      log_info "Check status: sudo systemctl status searxng"
    fi
  fi
}

configure_sx_for_searxng() {
  local sx_config_dir="${XDG_CONFIG_HOME}/sx"
  local sx_config_file="${sx_config_dir}/config.toml"

  mkdir -p "$sx_config_dir"

  if [[ -f "$sx_config_file" ]]; then
    # Update existing config - just change the URL
    if grep -q "searxng_url" "$sx_config_file"; then
      sed -i "s|searxng_url = .*|searxng_url = \"http://${SEARXNG_BIND}:${SEARXNG_PORT}\"|" "$sx_config_file"
      log_success "Updated sx config with local SearXNG URL"
    else
      echo "searxng_url = \"http://${SEARXNG_BIND}:${SEARXNG_PORT}\"" >>"$sx_config_file"
      log_success "Added SearXNG URL to sx config"
    fi
  else
    cat >"$sx_config_file" <<EOF
"\$schema" = "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/sx/sx.config.schema.json"

# sx configuration - generated by Octo setup.sh
engine = "searxng"
searxng_url = "http://${SEARXNG_BIND}:${SEARXNG_PORT}"
result_count = 10
safe_search = "none"
http_method = "GET"
timeout = 30.0
history_enabled = true
max_history = 100
EOF
    log_success "Created sx config at $sx_config_file"
  fi
}

# ==============================================================================
# EAVS Installation (LLM proxy for agents)
# ==============================================================================
#
# EAVS is a bidirectional LLM proxy that:
#   - Routes requests to multiple providers (Anthropic, OpenAI, Google, etc.)
#   - Manages virtual API keys per session with budgets and rate limits
#   - Provides a single endpoint for all LLM access
#   - Octo creates per-session virtual keys automatically
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

  install_rust_tool eavs

  if ! command_exists eavs; then
    log_error "EAVS installation failed"
    return 1
  fi

  log_success "EAVS installed: $(eavs --version 2>/dev/null | head -1)"
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

  # In multi-user mode, also enable gnome-keyring for the octo system user.
  # The EAVS service runs as octo and needs D-Bus + gnome-keyring for the
  # keychain: config syntax and `eavs secret set` commands.
  if [[ "$SELECTED_USER_MODE" == "multi" ]] && id octo &>/dev/null; then
    enable_keyring_for_octo_user
  fi
}

# Enable gnome-keyring-daemon for the octo system user so that:
#   1. The EAVS system service (User=octo) can resolve keychain: secrets at startup
#   2. Admins can run `sudo -u octo dbus-run-session -- eavs secret set <name>`
# Requires linger so octo's user-level systemd instance persists without a login.
enable_keyring_for_octo_user() {
  log_info "Enabling gnome-keyring for octo user..."

  # Enable linger so octo gets a persistent user-level systemd instance
  sudo loginctl enable-linger octo 2>/dev/null || true

  # Enable the gnome-keyring socket for the octo user
  if [[ -f /usr/lib/systemd/user/gnome-keyring-daemon.socket ]]; then
    sudo -u octo systemctl --user enable gnome-keyring-daemon.socket 2>/dev/null || true
    sudo -u octo systemctl --user start gnome-keyring-daemon.socket 2>/dev/null || true
    log_success "gnome-keyring-daemon.socket enabled for octo user"
  fi
}

configure_eavs() {
  log_step "Configuring EAVS"

  # Determine config/data paths based on user mode:
  #   single-user: ~/.config/eavs/ (runs as installing user)
  #   multi-user:  ~octo/.config/eavs/ (runs as octo system user, same home)
  local eavs_config_dir eavs_data_dir eavs_env_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_config_dir="${XDG_CONFIG_HOME}/eavs"
    eavs_data_dir="${XDG_DATA_HOME:-$HOME/.local/share}/eavs"
    eavs_env_file="${eavs_config_dir}/env"
    mkdir -p "$eavs_config_dir" "$eavs_data_dir"
  else
    eavs_config_dir="${OCTO_HOME}/.config/eavs"
    eavs_data_dir="${OCTO_HOME}/.local/share/eavs"
    eavs_env_file="${eavs_config_dir}/env"
    sudo mkdir -p "$eavs_config_dir" "$eavs_data_dir"
  fi

  local eavs_config_file="${eavs_config_dir}/config.toml"

  # Generate master key for octo to create per-session virtual keys
  EAVS_MASTER_KEY=$(generate_secure_secret 32)

  echo
  echo "EAVS needs at least one LLM provider to route agent requests."
  echo "You can add more providers later by editing: $eavs_config_file"
  echo

  # Collect provider configs
  local providers_toml=""
  local first_provider=""
  local has_any_provider="false"

  # Ask about each major provider
  for provider_name in anthropic openai google openrouter groq mistral; do
    local env_var_name=""
    local provider_type=""
    local display_name=""
    local signup_url=""

    case "$provider_name" in
    anthropic)
      env_var_name="ANTHROPIC_API_KEY"
      provider_type="anthropic"
      display_name="Anthropic (Claude)"
      signup_url="https://console.anthropic.com/"
      ;;
    openai)
      env_var_name="OPENAI_API_KEY"
      provider_type="openai"
      display_name="OpenAI (GPT)"
      signup_url="https://platform.openai.com/api-keys"
      ;;
    google)
      env_var_name="GEMINI_API_KEY"
      provider_type="google"
      display_name="Google (Gemini)"
      signup_url="https://aistudio.google.com/app/apikey"
      ;;
    openrouter)
      env_var_name="OPENROUTER_API_KEY"
      provider_type="openrouter"
      display_name="OpenRouter"
      signup_url="https://openrouter.ai/keys"
      ;;
    groq)
      env_var_name="GROQ_API_KEY"
      provider_type="groq"
      display_name="Groq"
      signup_url="https://console.groq.com/keys"
      ;;
    mistral)
      env_var_name="MISTRAL_API_KEY"
      provider_type="mistral"
      display_name="Mistral AI"
      signup_url="https://console.mistral.ai/"
      ;;
    esac

    # Check if key exists in environment
    local existing_key=""
    existing_key="${!env_var_name:-}"

    if [[ -n "$existing_key" ]]; then
      log_info "Found $env_var_name in environment"
      if confirm "Configure $display_name (key found in env)?"; then
        providers_toml+="
[providers.${provider_name}]
type = \"${provider_type}\"
api_key = \"env:${env_var_name}\"
"
        has_any_provider="true"
        if [[ -z "$first_provider" ]]; then
          first_provider="$provider_name"
        fi
      fi
    else
      if confirm "Configure $display_name?" "n"; then
        echo "  Get your API key from: $signup_url"
        local api_key
        api_key=$(prompt_input "  $display_name API key")
        if [[ -n "$api_key" ]]; then
          # Store key in env file, reference via env: syntax in config
          if [[ "$SELECTED_USER_MODE" == "single" ]]; then
            echo "${env_var_name}=${api_key}" >>"${eavs_env_file}"
          else
            echo "${env_var_name}=${api_key}" | sudo tee -a "${eavs_env_file}" >/dev/null
          fi
          providers_toml+="
[providers.${provider_name}]
type = \"${provider_type}\"
api_key = \"env:${env_var_name}\"
"
          has_any_provider="true"
          if [[ -z "$first_provider" ]]; then
            first_provider="$provider_name"
          fi
        fi
      fi
    fi
  done

  if [[ "$has_any_provider" != "true" ]]; then
    log_warn "No providers configured. EAVS will start but agents cannot use any LLM."
    log_info "Add providers later: edit $eavs_config_file"
  fi

  # Set first provider as default (EAVS routes requests to "default" when
  # no X-Provider header is sent)
  local default_provider_toml=""
  if [[ -n "$first_provider" ]]; then
    local first_type first_key
    first_type=$(grep "^type = " <<<"$providers_toml" | head -1 | cut -d'"' -f2)
    first_key=$(grep "^api_key = " <<<"$providers_toml" | head -1 | cut -d'"' -f2)
    default_provider_toml="[providers.default]
type = \"${first_type}\"
api_key = \"${first_key}\"
"
  fi

  # Write eavs config
  local config_content
  config_content=$(cat <<EOF
"\$schema" = "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/eavs/eavs.config.schema.json"

# EAVS Configuration - generated by Octo setup.sh
# Edit this file to add/change LLM providers.
# Docs: https://github.com/byteowlz/eavs

[server]
host = "127.0.0.1"
port = ${EAVS_PORT}

# --- Providers ---
# API key values support: "env:VAR_NAME", "keychain:account", or literal strings.
# Add providers with: eavs secret set <name>  (stores in system keychain)
# Then use: api_key = "keychain:<name>"

${default_provider_toml}
${providers_toml}

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
    # Append master key to env file
    echo "EAVS_MASTER_KEY=${EAVS_MASTER_KEY}" >>"${eavs_env_file}"
    chmod 600 "${eavs_env_file}"
  else
    echo "$config_content" | sudo tee "$eavs_config_file" >/dev/null
    echo "EAVS_MASTER_KEY=${EAVS_MASTER_KEY}" | sudo tee -a "${eavs_env_file}" >/dev/null
    sudo chmod 600 "${eavs_env_file}"
    # Owned by octo - same user that runs the eavs service
    sudo chown -R octo:octo "$eavs_config_dir" "$eavs_data_dir"
  fi

  log_success "EAVS config written to $eavs_config_file"
  if [[ -n "$first_provider" ]]; then
    log_success "Default provider: $first_provider"
  fi
}

install_eavs_service() {
  log_step "Setting up EAVS service"

  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    # Single-user: systemd user service, runs as installing user
    local eavs_config_dir="${XDG_CONFIG_HOME}/eavs"
    local service_dir="$HOME/.config/systemd/user"
    mkdir -p "$service_dir"

    cat >"${service_dir}/eavs.service" <<EOF
[Unit]
Description=EAVS LLM Proxy
After=default.target

[Service]
Type=simple
EnvironmentFile=-${eavs_config_dir}/env
ExecStart=/usr/local/bin/eavs serve
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
    # Multi-user: system service, runs as octo user alongside the backend.
    # EAVS config lives in octo's home (~octo/.config/eavs/) so XDG just works.
    #
    # The DBUS_SESSION_BUS_ADDRESS and XDG_RUNTIME_DIR environment variables
    # give the service access to the octo user's D-Bus session bus, which is
    # needed for gnome-keyring (the keychain: config syntax). This requires
    # linger to be enabled for octo (done in enable_keyring_for_octo_user).
    local octo_uid
    octo_uid=$(id -u octo)

    sudo tee /etc/systemd/system/eavs.service >/dev/null <<EOF
[Unit]
Description=EAVS LLM Proxy
After=network.target
Before=octo.service

[Service]
Type=simple
User=octo
Group=octo
WorkingDirectory=${OCTO_HOME}
Environment=HOME=${OCTO_HOME}
Environment=XDG_CONFIG_HOME=${OCTO_HOME}/.config
Environment=XDG_DATA_HOME=${OCTO_HOME}/.local/share
Environment=XDG_STATE_HOME=${OCTO_HOME}/.local/state
Environment=XDG_RUNTIME_DIR=/run/user/${octo_uid}
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${octo_uid}/bus
EnvironmentFile=-${OCTO_HOME}/.config/eavs/env
ExecStart=/usr/local/bin/eavs serve
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=${OCTO_HOME}
ReadWritePaths=/run/user/${octo_uid}
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

    sudo systemctl daemon-reload
    sudo systemctl enable eavs
    sudo systemctl start eavs
    log_success "EAVS started (system service, user=octo, port ${EAVS_PORT})"
  fi
}

# ==============================================================================
# Production Mode Setup
# ==============================================================================

select_deployment_mode() {
  log_step "Deployment Mode Selection"

  echo
  echo "Octo can be deployed in two modes:"
  echo
  echo -e "  ${BOLD}Development${NC} - For local development and testing"
  echo "    - Uses dev_mode authentication (no JWT secret required)"
  echo "    - Preconfigured dev users for easy login"
  echo "    - HTTP only (no TLS)"
  echo "    - Best for: local development, testing"
  echo
  echo -e "  ${BOLD}Production${NC} - For server deployments"
  echo "    - Secure JWT-based authentication"
  echo "    - Creates an admin user with secure credentials"
  echo "    - Optional Caddy reverse proxy with automatic HTTPS"
  echo "    - Best for: servers, multi-user deployments, remote access"
  echo

  local choice
  choice=$(prompt_choice "Select deployment mode:" "Development" "Production")

  case "$choice" in
  "Development")
    PRODUCTION_MODE="false"
    OCTO_DEV_MODE="true"
    log_info "Development mode selected"
    ;;
  "Production")
    PRODUCTION_MODE="true"
    OCTO_DEV_MODE="false"
    log_info "Production mode selected"
    setup_production_mode
    ;;
  esac
}

setup_production_mode() {
  log_step "Production Mode Configuration"

  # Generate JWT secret
  echo
  log_info "Generating secure JWT secret..."
  JWT_SECRET=$(generate_secure_secret 64)
  log_success "JWT secret generated (64 characters)"

  # Admin user setup
  setup_admin_user

  # Caddy setup
  setup_caddy_prompt
}

generate_secure_secret() {
  local length="${1:-64}"
  # Use openssl for cryptographically secure random bytes
  if command_exists openssl; then
    openssl rand -base64 "$((length * 3 / 4))" | tr -d '/+=' | head -c "$length"
  else
    # Fallback to /dev/urandom
    head -c "$((length * 2))" /dev/urandom | base64 | tr -d '/+=' | head -c "$length"
  fi
}

setup_admin_user() {
  log_step "Admin User Setup"

  echo
  echo "Create an administrator account to manage Octo."
  echo "This user will be able to:"
  echo "  - Access the admin dashboard"
  echo "  - Create invite codes for new users"
  echo "  - Manage sessions and users"
  echo

  # Username
  ADMIN_USERNAME=$(prompt_input "Admin username" "admin")

  # Email
  ADMIN_EMAIL=$(prompt_input "Admin email" "admin@localhost")

  # Password
  echo
  if [[ "$NONINTERACTIVE" == "true" ]]; then
    ADMIN_PASSWORD=$(generate_secure_secret 16)
    log_info "Generated admin password: $ADMIN_PASSWORD"
    log_warn "SAVE THIS PASSWORD - it will not be shown again!"
  else
    while true; do
      ADMIN_PASSWORD=$(prompt_password "Admin password (min 8 characters)")
      if [[ ${#ADMIN_PASSWORD} -lt 8 ]]; then
        log_error "Password must be at least 8 characters"
        continue
      fi

      local confirm_password
      confirm_password=$(prompt_password "Confirm password")

      if [[ "$ADMIN_PASSWORD" != "$confirm_password" ]]; then
        log_error "Passwords do not match"
        continue
      fi

      break
    done
  fi

  log_success "Admin user configured: $ADMIN_USERNAME"
}

setup_caddy_prompt() {
  log_step "Reverse Proxy Setup (Caddy)"

  echo
  echo "Caddy provides a reverse proxy with automatic HTTPS."
  echo "This is recommended for production deployments."
  echo
  echo "Features:"
  echo "  - Automatic TLS certificate from Let's Encrypt"
  echo "  - HTTP/2 support"
  echo "  - Simple configuration"
  echo

  if [[ -n "$OCTO_SETUP_CADDY" ]]; then
    SETUP_CADDY="$OCTO_SETUP_CADDY"
  elif confirm "Set up Caddy reverse proxy?" "y"; then
    SETUP_CADDY="yes"
  else
    SETUP_CADDY="no"
  fi

  if [[ "$SETUP_CADDY" == "yes" ]]; then
    setup_caddy_config
  fi
}

setup_caddy_config() {
  echo
  echo "Caddy requires a domain name for HTTPS certificates."
  echo "The domain must point to this server's IP address."
  echo
  echo "Examples:"
  echo "  - octo.example.com"
  echo "  - agents.mycompany.io"
  echo "  - localhost (for local testing without TLS)"
  echo

  if [[ -n "$OCTO_DOMAIN" ]]; then
    DOMAIN="$OCTO_DOMAIN"
  else
    DOMAIN=$(prompt_input "Domain name" "localhost")
  fi

  if [[ "$DOMAIN" == "localhost" ]]; then
    log_warn "Using localhost - HTTPS will not be enabled"
  else
    log_info "Caddy will obtain TLS certificate for: $DOMAIN"
  fi
}

install_caddy() {
  if [[ "$SETUP_CADDY" != "yes" ]]; then
    return 0
  fi

  log_step "Installing Caddy"

  if command_exists caddy; then
    log_success "Caddy already installed: $(caddy version 2>/dev/null | head -1)"
    return 0
  fi

  case "$OS" in
  macos)
    if command_exists brew; then
      log_info "Installing Caddy via Homebrew..."
      brew install caddy
    else
      log_warn "Homebrew not found. Please install Caddy manually:"
      log_info "  brew install caddy"
      log_info "  or download from: https://caddyserver.com/download"
    fi
    ;;
  linux)
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros)
      log_info "Installing Caddy via pacman..."
      sudo pacman -S --noconfirm caddy
      ;;
    debian | ubuntu | pop | linuxmint)
      log_info "Installing Caddy via apt..."
      sudo apt-get install -y debian-keyring debian-archive-keyring apt-transport-https curl
      curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
      curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
      apt_update_once force
      sudo apt-get install -y caddy
      ;;
    fedora)
      log_info "Installing Caddy via dnf..."
      sudo dnf install -y 'dnf-command(copr)'
      sudo dnf copr enable -y @caddy/caddy
      sudo dnf install -y caddy
      ;;
    *)
      log_warn "Unknown distribution. Installing Caddy via GitHub release..."
      install_caddy_binary
      ;;
    esac
    ;;
  esac

  if command_exists caddy; then
    log_success "Caddy installed successfully"
  else
    log_warn "Caddy installation may have failed. Please install manually."
  fi
}

install_caddy_binary() {
  local caddy_version="2.9.1"
  local arch="$ARCH"

  case "$arch" in
  x86_64) arch="amd64" ;;
  aarch64) arch="arm64" ;;
  esac

  local caddy_url="https://github.com/caddyserver/caddy/releases/download/v${caddy_version}/caddy_${caddy_version}_linux_${arch}.tar.gz"

  log_info "Downloading Caddy ${caddy_version}..."
  curl -sL "$caddy_url" | sudo tar -xzC /usr/local/bin caddy
  sudo chmod +x /usr/local/bin/caddy
}

generate_caddyfile() {
  if [[ "$SETUP_CADDY" != "yes" ]]; then
    return 0
  fi

  log_step "Generating Caddyfile"

  local caddy_config_dir="/etc/caddy"
  local caddyfile="${caddy_config_dir}/Caddyfile"

  # Determine ports based on backend mode
  local backend_port="8080"
  local frontend_port="3000"

  # Create config directory
  if [[ ! -d "$caddy_config_dir" ]]; then
    sudo mkdir -p "$caddy_config_dir"
  fi

  # Generate Caddyfile
  #
  # Route structure:
  # - /api/* -> backend (strip /api prefix)
  # - /ws    -> backend WebSocket
  # - /session/* -> backend (terminal, files, code proxies)
  # - /health, /auth/*, /me, /admin/* -> backend
  # - Everything else -> frontend
  #
  if [[ "$DOMAIN" == "localhost" ]]; then
    # Local development - no TLS
    sudo tee "$caddyfile" >/dev/null <<EOF
# Octo Caddyfile - Local Development
# Generated by setup.sh on $(date)

:80 {
    # Backend API - strip /api prefix
    handle /api/* {
        uri strip_prefix /api
        reverse_proxy localhost:${backend_port}
    }
    
    # Backend WebSocket endpoint
    handle /ws {
        reverse_proxy localhost:${backend_port}
    }
    
    # Session proxies (terminal, files, code)
    handle /session/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Auth endpoints
    handle /auth/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Health check
    handle /health {
        reverse_proxy localhost:${backend_port}
    }
    
    # User profile
    handle /me {
        reverse_proxy localhost:${backend_port}
    }
    
    # Admin routes
    handle /admin/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Voice endpoints
    handle /voice/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Default to frontend
    handle {
        reverse_proxy localhost:${frontend_port}
    }
    
    log {
        output file /var/log/caddy/octo.log
    }
}
EOF
  else
    # Production - with TLS
    sudo tee "$caddyfile" >/dev/null <<EOF
# Octo Caddyfile - Production
# Generated by setup.sh on $(date)
# Domain: ${DOMAIN}

${DOMAIN} {
    # Backend API - strip /api prefix
    # Frontend calls /api/sessions -> backend /sessions
    handle /api/* {
        uri strip_prefix /api
        reverse_proxy localhost:${backend_port}
    }
    
    # Backend WebSocket endpoint
    handle /ws {
        reverse_proxy localhost:${backend_port}
    }
    
    # Session proxies (terminal WebSocket, files, code)
    handle /session/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Auth endpoints (login, register, logout)
    handle /auth/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Health check
    handle /health {
        reverse_proxy localhost:${backend_port}
    }
    
    # User profile endpoint
    handle /me {
        reverse_proxy localhost:${backend_port}
    }
    
    # Admin routes
    handle /admin/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Voice STT/TTS WebSocket endpoints
    handle /voice/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Workspace terminal (by path)
    handle /workspace/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Frontend (React app) - default handler
    handle {
        reverse_proxy localhost:${frontend_port}
    }
    
    # Security headers
    header {
        X-Content-Type-Options nosniff
        X-Frame-Options DENY
        Referrer-Policy strict-origin-when-cross-origin
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        X-XSS-Protection "1; mode=block"
        -Server
    }
    
    log {
        output file /var/log/caddy/octo.log
        format json
    }
    
    # Enable compression
    encode gzip zstd
}

# Redirect HTTP to HTTPS
http://${DOMAIN} {
    redir https://${DOMAIN}{uri} permanent
}
EOF
  fi

  # Create log directory
  sudo mkdir -p /var/log/caddy

  log_success "Caddyfile generated: $caddyfile"
}

install_caddy_service() {
  if [[ "$SETUP_CADDY" != "yes" ]]; then
    return 0
  fi

  log_step "Installing Caddy service"

  case "$OS" in
  linux)
    # Caddy usually comes with systemd service, but ensure it's enabled
    if [[ -f /lib/systemd/system/caddy.service ]] || [[ -f /etc/systemd/system/caddy.service ]]; then
      log_info "Enabling Caddy service..."
      sudo systemctl daemon-reload
      sudo systemctl enable caddy

      if confirm "Start Caddy now?"; then
        sudo systemctl start caddy
        log_success "Caddy service started"
        log_info "Check status with: sudo systemctl status caddy"
      fi
    else
      log_warn "Caddy systemd service not found. You may need to configure it manually."
    fi
    ;;
  macos)
    log_info "On macOS, Caddy can be started with:"
    log_info "  sudo caddy start --config /etc/caddy/Caddyfile"
    log_info "Or use Homebrew services:"
    log_info "  brew services start caddy"
    ;;
  esac
}

build_octo() {
  log_step "Building Octo components"

  cd "$SCRIPT_DIR"

  # Build backend (includes octo, octo-runner, octo-sandbox, pi-bridge binaries)
  log_info "Building backend..."
  (cd backend && cargo build --release)
  log_success "Backend built"

  # Build fileserver
  log_info "Building fileserver..."
  (cd fileserver && cargo build --release)
  log_success "Fileserver built"

  # Build frontend
  log_info "Installing frontend dependencies..."
  (cd frontend && bun install)
  log_info "Building frontend..."
  (cd frontend && bun run build)
  log_success "Frontend built"

  # Install binaries to /usr/local/bin (globally accessible)
  log_info "Installing binaries to ${TOOLS_INSTALL_DIR}..."

  local release_dir="$SCRIPT_DIR/backend/target/release"
  for bin in octo octo-runner pi-bridge octo-sandbox octo-setup; do
    if [[ -f "${release_dir}/${bin}" ]]; then
      sudo install -m 755 "${release_dir}/${bin}" "${TOOLS_INSTALL_DIR}/${bin}"
      log_success "${bin} installed"
    fi
  done

  if [[ -f "$SCRIPT_DIR/fileserver/target/release/fileserver" ]]; then
    sudo install -m 755 "$SCRIPT_DIR/fileserver/target/release/fileserver" "${TOOLS_INSTALL_DIR}/fileserver"
    log_success "fileserver installed"
  fi

  log_success "Binaries installed to ${TOOLS_INSTALL_DIR}"
}

# ==============================================================================
# Mode Selection
# ==============================================================================

select_user_mode() {
  log_step "User Mode Selection"

  echo
  echo "Octo supports two user modes:"
  echo
  echo -e "  ${BOLD}Single-user${NC} - Personal deployment"
  echo "    - All sessions use the same workspace"
  echo "    - Simpler setup, no user management"
  echo "    - Best for: personal laptops, single-developer servers"
  echo
  echo -e "  ${BOLD}Multi-user${NC} - Team deployment"
  echo "    - Each user gets isolated workspace"
  echo "    - User authentication and management"
  echo "    - Best for: teams, shared servers"

  if [[ "$OS" == "macos" ]]; then
    echo
    echo -e "  ${YELLOW}Note: Multi-user on macOS requires Docker/Podman${NC}"
  fi

  local choice
  choice=$(prompt_choice "Select user mode:" "Single-user" "Multi-user")

  case "$choice" in
  "Single-user")
    SELECTED_USER_MODE="single"
    ;;
  "Multi-user")
    SELECTED_USER_MODE="multi"
    # macOS multi-user requires container mode
    if [[ "$OS" == "macos" ]]; then
      log_info "Multi-user on macOS requires container mode"
      SELECTED_BACKEND_MODE="container"
    fi
    ;;
  esac

  log_info "Selected user mode: $SELECTED_USER_MODE"
}

select_backend_mode() {
  log_step "Backend Mode Selection"

  # If already set (e.g., macOS multi-user), skip
  if [[ -n "${SELECTED_BACKEND_MODE:-}" ]]; then
    log_info "Backend mode pre-selected: $SELECTED_BACKEND_MODE"
    return
  fi

  echo
  echo "Octo can run agents in two modes:"
  echo
  echo -e "  ${BOLD}Local${NC} - Native processes"
  echo "    - Runs Pi, fileserver, ttyd directly on host"
  echo "    - Lower overhead, faster startup"
  echo "    - Best for: development, single-user, trusted environments"
  echo
  echo -e "  ${BOLD}Container${NC} - Docker/Podman containers"
  echo "    - Full isolation per session"
  echo "    - Reproducible environment"
  echo "    - Best for: multi-user, production, untrusted code"

  local choice
  choice=$(prompt_choice "Select backend mode:" "Local" "Container")

  case "$choice" in
  "Local")
    SELECTED_BACKEND_MODE="local"
    ;;
  "Container")
    SELECTED_BACKEND_MODE="container"
    ;;
  esac

  log_info "Selected backend mode: $SELECTED_BACKEND_MODE"
}

# ==============================================================================
# Configuration Generation
# ==============================================================================

generate_jwt_secret() {
  openssl rand -base64 48
}

generate_password_hash() {
  local password="$1"
  # Use htpasswd if available, otherwise python
  if command_exists htpasswd; then
    htpasswd -nbBC 12 user "$password" | cut -d: -f2
  elif command_exists python3; then
    python3 -c "import bcrypt; print(bcrypt.hashpw('$password'.encode(), bcrypt.gensalt(12)).decode())"
  else
    log_error "Cannot generate password hash. Install htpasswd or python3 with bcrypt."
    exit 1
  fi
}

write_skdlr_agent_config() {
  local skdlr_config="/etc/octo/skdlr-agent.toml"
  local sandbox_config="/etc/octo/sandbox.toml"

  log_info "Writing skdlr agent config to $skdlr_config"

  sudo mkdir -p /etc/octo

  # Ensure sandbox config exists for octo-sandbox
  if [[ ! -f "$sandbox_config" ]]; then
    log_info "Creating default sandbox config at $sandbox_config"
    sudo cp "$SCRIPT_DIR/backend/crates/octo/examples/sandbox.toml" "$sandbox_config"
    sudo chmod 644 "$sandbox_config"
  fi

  sudo tee "$skdlr_config" >/dev/null <<'EOF'
# skdlr config for Octo sandboxed agents
# Forces all scheduled commands through octo-sandbox

[executor]
wrapper = "octo-sandbox"
wrapper_args = ["--config", "/etc/octo/sandbox.toml", "--workspace", "{workdir}", "--"]
EOF

  sudo chmod 644 "$skdlr_config"
}

generate_config() {
  log_step "Generating configuration"

  # Create config directories
  mkdir -p "$OCTO_CONFIG_DIR"
  mkdir -p "$OCTO_DATA_DIR"

  local config_file="$OCTO_CONFIG_DIR/config.toml"

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
  log_info "Configuring Octo..."

  # Workspace directory
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    WORKSPACE_DIR=$(prompt_input "Workspace directory" "$HOME/octo/workspace")
  else
    WORKSPACE_DIR=$(prompt_input "Workspace base directory (user dirs created here)" "$HOME/octo/{user_id}")
  fi

  # Auth configuration
  local dev_user_id dev_user_name dev_user_email dev_user_password dev_user_hash
  local admin_user_hash=""

  if [[ "$OCTO_DEV_MODE" == "true" ]]; then
    log_info "Setting up development user..."
    dev_user_id=$(prompt_input "Dev user ID" "dev")
    dev_user_name=$(prompt_input "Dev user name" "Developer")
    dev_user_email=$(prompt_input "Dev user email" "dev@localhost")
    dev_user_password=$(prompt_password "Dev user password")

    if [[ -n "$dev_user_password" ]]; then
      log_info "Generating password hash..."
      dev_user_hash=$(generate_password_hash "$dev_user_password")
    else
      dev_user_hash=""
    fi
  elif [[ "$PRODUCTION_MODE" == "true" ]]; then
    # Production mode - use the admin user configured earlier
    log_info "Generating admin user password hash..."
    admin_user_hash=$(generate_password_hash "$ADMIN_PASSWORD")
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
    echo "  - The 'octo' group will be created"
    echo "  - Sudoers rules will allow managing octo_* users"
    echo
    if confirm "Enable Linux user isolation? (requires sudo)"; then
      linux_users_enabled="true"
      LINUX_USERS_ENABLED="true"
    fi
  fi

  # Write config file
  log_info "Writing config to $config_file"

  cat >"$config_file" <<EOF
# Octo Configuration
# Generated by setup.sh on $(date)

"\$schema" = "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/octo/octo.backend.config.schema.json"

profile = "default"

[logging]
level = "$OCTO_LOG_LEVEL"

[runtime]
timeout = 60
fail_fast = true

[backend]
mode = "$SELECTED_BACKEND_MODE"

[container]
runtime = "${CONTAINER_RUNTIME:-docker}"
default_image = "octo-dev:latest"
base_port = 41820
EOF

  if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
    cat >>"$config_file" <<EOF

[local]
enabled = true
fileserver_binary = "fileserver"
ttyd_binary = "ttyd"
workspace_dir = "$WORKSPACE_DIR"
single_user = $([[ "$SELECTED_USER_MODE" == "single" ]] && echo "true" || echo "false")

[local.linux_users]
enabled = $linux_users_enabled
prefix = "octo_"
uid_start = 2000
group = "octo"
shell = "/bin/bash"
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
dev_mode = $OCTO_DEV_MODE
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

  if [[ "$OCTO_DEV_MODE" == "true" && -n "${dev_user_hash:-}" ]]; then
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
  # Determine Pi runtime mode based on backend mode and user mode
  local pi_runtime_mode="local"
  if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
    pi_runtime_mode="container"
  elif [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
    pi_runtime_mode="runner"
  fi

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
subdirectory = "onboarding"
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
public_dropbox = "${FEEDBACK_PUBLIC_DROPBOX:-/usr/local/share/octo/issues}"
private_archive = "${FEEDBACK_PRIVATE_ARCHIVE:-/var/lib/octo/issue-archive}"
keep_public = true
sync_interval_seconds = 60
EOF

  # Add runner socket pattern for multi-user Linux mode
  if [[ "$pi_runtime_mode" == "runner" ]]; then
    cat >>"$config_file" <<'EOF'

[local]
runner_socket_pattern = "/run/octo/runner-sockets/{user}/octo-runner.sock"
EOF
  fi

  cat >>"$config_file" <<EOF

[sessions]
auto_attach = "off"
auto_attach_scan = true

[scaffold]
binary = "byt"
subcommand = "new"
template_arg = "--template"
output_arg = "--output"
github_arg = "--github"
private_arg = "--private"
description_arg = "--description"
EOF

  log_success "Configuration written to $config_file"

  # API keys are now managed by EAVS, not stored in octo's env file

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
    local creds_file="$OCTO_CONFIG_DIR/.admin_setup"
    cat >"$creds_file" <<EOF
ADMIN_USERNAME="$ADMIN_USERNAME"
ADMIN_EMAIL="$ADMIN_EMAIL"
ADMIN_PASSWORD_HASH="$admin_user_hash"
EOF
    chmod 600 "$creds_file"
    log_info "Admin credentials saved for database setup"
  fi
}

# ==============================================================================
# Linux User Isolation Setup
# ==============================================================================

# Global variable to track if linux user isolation is enabled
LINUX_USERS_ENABLED="false"

setup_linux_user_isolation() {
  if [[ "$LINUX_USERS_ENABLED" != "true" ]]; then
    return 0
  fi

  log_step "Setting up Linux user isolation"

  local octo_group="octo"
  local user_prefix="octo_"
  local server_user

  # Determine who will run the backend:
  # - For system service (multi-user production): use 'octo' system user
  # - For user service (development): use current user
  if [[ "${MULTI_USER:-false}" == "true" ]] && [[ "$OS" == "linux" ]]; then
    # Production multi-user mode: backend runs as 'octo' system user
    server_user="octo"
    ensure_octo_system_user
  else
    # Development mode: backend runs as current user
    server_user=$(whoami)
  fi

  log_info "Sudoers rules will be configured for user: $server_user"

  # 1. Create the octo group
  if ! getent group "$octo_group" &>/dev/null; then
    log_info "Creating group '$octo_group'..."
    if ! sudo groupadd "$octo_group"; then
      log_error "Failed to create group '$octo_group'"
      return 1
    fi
    log_success "Group '$octo_group' created"
  else
    log_success "Group '$octo_group' already exists"
  fi

  # 2. Add the server user to the octo group
  log_info "Adding user '$server_user' to group '$octo_group'..."
  if ! sudo usermod -aG "$octo_group" "$server_user"; then
    log_warn "Failed to add user to group (may need to re-login)"
  else
    log_success "User '$server_user' added to group '$octo_group'"
  fi

  # 3. Create sudoers file for multi-user process management
  log_info "Configuring sudoers for multi-user process management..."

  local sudoers_file="/etc/sudoers.d/octo-multiuser"
  # Note: uid_start comes from config, default 2000
  local uid_start="${OCTO_UID_START:-2000}"
  # Extract first digit of uid_start for regex (assumes 4-digit UIDs starting with 2-9)
  local uid_first_digit="${uid_start:0:1}"

  local sudoers_content="# Octo Multi-User Process Isolation - SECURE VERSION
# Generated by setup.sh on $(date)
# Allows the octo server user to manage isolated user accounts
#
# SECURITY: Uses regex patterns (^...\$) to prevent privilege escalation.
# - UIDs restricted to ${uid_first_digit}000-${uid_first_digit}999 range (avoids system/user UIDs)
# - Usernames must start with ${user_prefix} prefix
# - Workspace chown restricted to ${user_prefix}* home directories only
# Requires sudo 1.9.10+ for regex support.

# Group management - only create the ${octo_group} group (safe - fixed value)
Cmnd_Alias OCTO_GROUPADD = /usr/sbin/groupadd ${octo_group}

# User creation - RESTRICTED to safe UID range and ${user_prefix} prefix
# Regex matches: -u NNNN -g ${octo_group} -s /bin/bash -m/-M -c COMMENT USERNAME
# UID must be ${uid_first_digit}000-${uid_first_digit}999, username must start with ${user_prefix}
# GECOS format: "Octo platform user: <user_id >" - use .* to match including spaces
Cmnd_Alias OCTO_USERADD = \\
    /usr/sbin/useradd ^-u [${uid_first_digit}][0-9][0-9][0-9] -g ${octo_group} -s /bin/bash -m -c .* ${user_prefix}[a-z0-9_-]+\$, \\
    /usr/sbin/useradd ^-u [${uid_first_digit}][0-9][0-9][0-9] -g ${octo_group} -s /bin/bash -M -c .* ${user_prefix}[a-z0-9_-]+\$

# User deletion - only ${user_prefix} users, no home removal (-r flag not allowed)
Cmnd_Alias OCTO_USERDEL = /usr/sbin/userdel ^${user_prefix}[a-z0-9_-]+\$

# Directory creation for runner sockets - RESTRICTED path (no path traversal)
Cmnd_Alias OCTO_MKDIR = /bin/mkdir ^-p /run/octo/runner-sockets/${user_prefix}[a-z0-9_-]+\$

# Runner socket ownership - RESTRICTED to exact paths
Cmnd_Alias OCTO_CHOWN_RUNNER = \\
    /usr/bin/chown ^${user_prefix}[a-z0-9_-]+\\:${octo_group} /run/octo/runner-sockets/${user_prefix}[a-z0-9_-]+\$

# Workspace ownership - RESTRICTED to ${user_prefix} user home directories ONLY
# SECURITY: Only allows chown on /home/${user_prefix}*/... NOT on other users' homes
# The regex ensures the path starts with /home/${user_prefix} to prevent privilege escalation
Cmnd_Alias OCTO_CHOWN_WORKSPACE = \\
    /usr/bin/chown ^-R ${user_prefix}[a-z0-9_-]+\\:${octo_group} /home/${user_prefix}[a-z0-9_-]+(/[^.][^/]*)*\$

# Permissions for runner socket directories
Cmnd_Alias OCTO_CHMOD_RUNNER = /usr/bin/chmod ^2770 /run/octo/runner-sockets/${user_prefix}[a-z0-9_-]+\$

# systemd linger - only for ${user_prefix} users
Cmnd_Alias OCTO_LINGER = /usr/bin/loginctl ^enable-linger ${user_prefix}[a-z0-9_]+\$

# Start user systemd instance - RESTRICTED to ${user_prefix} user UIDs
Cmnd_Alias OCTO_START_USER = /usr/bin/systemctl ^start user@[${uid_first_digit}][0-9][0-9][0-9]\\.service\$

# User management - group and user creation
${server_user} ALL=(root) NOPASSWD: OCTO_GROUPADD, OCTO_USERADD

# systemd user management - enable/start octo-runner as ${user_prefix}* users
Cmnd_Alias OCTO_RUNNER_SYSTEMCTL = \\
    /usr/bin/systemctl --user enable --now octo-runner, \\
    /usr/bin/systemctl --user start octo-runner, \\
    /usr/bin/systemctl --user enable octo-runner
${server_user} ALL=(${user_prefix}*) NOPASSWD: OCTO_RUNNER_SYSTEMCTL

# Runner socket directory setup and workspace ownership
${server_user} ALL=(root) NOPASSWD: OCTO_MKDIR, OCTO_CHOWN_RUNNER, OCTO_CHOWN_WORKSPACE, OCTO_CHMOD_RUNNER

# User systemd management
${server_user} ALL=(root) NOPASSWD: OCTO_START_USER, OCTO_LINGER
"

  # Write sudoers file (use visudo -c to validate)
  echo "$sudoers_content" | sudo tee "$sudoers_file" >/dev/null
  sudo chmod 440 "$sudoers_file"

  # Validate the sudoers file
  if sudo visudo -c -f "$sudoers_file" &>/dev/null; then
    log_success "Sudoers file created: $sudoers_file"
  else
    log_error "Invalid sudoers file - removing it"
    sudo rm -f "$sudoers_file"
    return 1
  fi

  # 4. Create workspace base directory with proper permissions
  log_info "Creating workspace directory structure..."
  local workspace_base="/var/lib/octo/workspaces"
  sudo mkdir -p "$workspace_base"
  sudo chown root:"$octo_group" "$workspace_base"
  sudo chmod 775 "$workspace_base"
  log_success "Workspace directory created: $workspace_base"

  # 5. Install system sandbox config (trusted, root-owned)
  log_info "Installing system sandbox configuration..."
  local sandbox_config="/etc/octo/sandbox.toml"
  sudo mkdir -p /etc/octo

  sudo tee "$sandbox_config" >/dev/null <<'EOF'
# Octo Sandbox Configuration (System-wide)
# This file is owned by root and trusted by octo-runner.
# It cannot be modified by regular users or compromised agents.

enabled = true
profile = "development"

# Paths to deny read access (sensitive files)
deny_read = [
    "~/.ssh",
    "~/.gnupg",
    "~/.aws",
    "~/.config/gcloud",
    "~/.kube",
    "/usr/bin/systemctl",
    "/bin/systemctl",
    "/usr/bin/systemd-run",
    "/bin/systemd-run",
]

# Paths to allow write access (in addition to workspace)
allow_write = [
    # Package managers / toolchains
    "~/.cargo",
    "~/.rustup",
    "~/.npm",
    "~/.bun",
    "~/.local/bin",
    # Agent tools - data directories
    "~/.local/share/skdlr",
    "~/.local/share/mmry",
    # Agent tools - config directories
    "~/.config/skdlr",
    "~/.config/mmry",
    "~/.config/byt",
    "/tmp",
]

# Paths to deny write access (takes precedence)
deny_write = [
    "/etc/octo/sandbox.toml",
]

# Namespace isolation
isolate_network = false
isolate_pid = true
EOF

  sudo chmod 644 "$sandbox_config"
  sudo chown root:root "$sandbox_config"
  log_success "System sandbox config installed: $sandbox_config"

  # 6. Install skdlr agent config (forces octo-sandbox wrapper)
  log_info "Installing skdlr agent configuration..."
  local skdlr_config="/etc/octo/skdlr-agent.toml"

  sudo tee "$skdlr_config" >/dev/null <<'EOF'
# Octo skdlr configuration for agent scheduling
# This file is owned by root and enforces octo-sandbox for scheduled runs.

[executor]
wrapper = "octo-sandbox"
wrapper_args = [
    "--config", "/etc/octo/sandbox.toml",
    "--workspace", "{workdir}",
    "--"
]
EOF

  sudo chmod 644 "$skdlr_config"
  sudo chown root:root "$skdlr_config"
  log_success "Skdlr agent config installed: $skdlr_config"

  echo
  log_success "Linux user isolation configured successfully"
  echo
  echo "The following has been set up:"
  echo "  - Group '$octo_group' created"
  echo "  - User '$server_user' added to group '$octo_group'"
  echo "  - Sudoers rules for user management: $sudoers_file"
  echo "  - Workspace directory: $workspace_base"
  echo "  - System sandbox config: $sandbox_config"
  echo
  echo "When users are created, the server will:"
  echo "  1. Create a Linux user (e.g., ${user_prefix}1, ${user_prefix}2, ...)"
  echo "  2. Create their home directory with isolated workspace"
  echo "  3. Run their agent processes as that user (sandboxed)"
  echo
  echo "Security features:"
  echo "  - Processes run in bubblewrap sandbox with namespace isolation"
  echo "  - Sensitive paths (~/.ssh, ~/.aws, etc.) are blocked"
  echo "  - Sandbox config is root-owned and cannot be modified by agents"
  echo
  log_warn "You may need to log out and back in for group membership to take effect"
}

# ==============================================================================
# Admin User Database Setup
# ==============================================================================

create_admin_user_db() {
  if [[ "$PRODUCTION_MODE" != "true" ]]; then
    return 0
  fi

  log_step "Creating admin user in database"

  local creds_file="$OCTO_CONFIG_DIR/.admin_setup"
  if [[ ! -f "$creds_file" ]]; then
    log_warn "Admin credentials file not found, skipping database setup"
    return 0
  fi

  # shellcheck source=/dev/null
  source "$creds_file"

  # Check if octo CLI is available
  if ! command_exists octo; then
    log_warn "octo CLI not found. You'll need to create the admin user manually."
    log_info "After starting the server, run:"
    log_info "  octo user create --username \"$ADMIN_USERNAME\" --email \"$ADMIN_EMAIL\" --role admin"
    return 0
  fi

  echo
  echo "The admin user will be created when you first start Octo."
  echo "You can also create users manually with:"
  echo "  octo user create --username \"$ADMIN_USERNAME\" --email \"$ADMIN_EMAIL\" --role admin"
  echo

  # Generate an initial invite code for the admin
  generate_initial_invite_code

  # Clean up the credentials file
  rm -f "$creds_file"
}

generate_initial_invite_code() {
  log_step "Generating initial invite code"

  echo
  echo "To add additional users, you'll need invite codes."
  echo "An initial invite code will be generated when you start Octo."
  echo
  echo "After starting the server, create invite codes with:"
  echo "  octo invites create --uses 1"
  echo
  echo "Or use the web admin interface at:"
  if [[ -n "$DOMAIN" && "$DOMAIN" != "localhost" ]]; then
    echo "  https://${DOMAIN}/admin"
  else
    echo "  http://localhost:8080/admin"
  fi
}

# ==============================================================================
# Server Hardening (Linux only)
# ==============================================================================

# Check if we should run hardening
should_harden_server() {
  # Only on Linux
  [[ "$OS" != "linux" ]] && return 1

  # Only in production mode
  [[ "$PRODUCTION_MODE" != "true" ]] && return 1

  # Check if explicitly set
  if [[ "$OCTO_HARDEN_SERVER" == "yes" ]]; then
    return 0
  elif [[ "$OCTO_HARDEN_SERVER" == "no" ]]; then
    return 1
  fi

  # Prompt user
  if [[ "$NONINTERACTIVE" != "true" ]]; then
    if confirm "Apply server hardening (firewall, fail2ban, SSH hardening)?"; then
      OCTO_HARDEN_SERVER="yes"
      return 0
    fi
  fi

  return 1
}

# Install security packages
install_security_packages() {
  log_step "Installing security packages"

  case "$OS_DISTRO" in
  debian | ubuntu | pop | linuxmint)
    log_info "Installing security packages via apt..."
    apt_update_once
    sudo apt-get install -y \
      ufw \
      fail2ban \
      unattended-upgrades \
      apt-listchanges \
      logwatch \
      auditd
    ;;
  fedora | centos | rhel | rocky | alma)
    log_info "Installing security packages via dnf..."
    sudo dnf install -y \
      firewalld \
      fail2ban \
      dnf-automatic \
      audit
    ;;
  arch | manjaro | endeavouros)
    log_info "Installing security packages via pacman..."
    sudo pacman -S --noconfirm \
      ufw \
      fail2ban \
      audit
    ;;
  opensuse* | suse*)
    log_info "Installing security packages via zypper..."
    sudo zypper install -y \
      firewalld \
      fail2ban \
      audit
    ;;
  *)
    log_warn "Unknown distribution: $OS_DISTRO. Skipping security package installation."
    log_info "Please install manually: ufw/firewalld, fail2ban, auditd"
    return 1
    ;;
  esac

  log_success "Security packages installed"
}

# Configure firewall (UFW or firewalld)
configure_firewall() {
  if [[ "$OCTO_SETUP_FIREWALL" != "yes" ]]; then
    log_info "Skipping firewall configuration"
    return
  fi

  log_step "Configuring firewall"

  local ssh_port="${OCTO_SSH_PORT:-22}"
  local http_port="80"
  local https_port="443"
  local octo_port="8080"
  local frontend_port="3000"

  case "$OS_DISTRO" in
  debian | ubuntu | pop | linuxmint | arch | manjaro | endeavouros)
    if command_exists ufw; then
      log_info "Configuring UFW firewall..."

      # Set default policies
      sudo ufw default deny incoming
      sudo ufw default allow outgoing

      # Allow SSH (important: do this first!)
      sudo ufw allow "$ssh_port/tcp" comment 'SSH'

      # Allow HTTP/HTTPS for Caddy
      if [[ "$SETUP_CADDY" == "yes" ]]; then
        sudo ufw allow "$http_port/tcp" comment 'HTTP'
        sudo ufw allow "$https_port/tcp" comment 'HTTPS'
      fi

      # Allow Octo ports (only if not using Caddy)
      if [[ "$SETUP_CADDY" != "yes" ]]; then
        sudo ufw allow "$octo_port/tcp" comment 'Octo API'
        sudo ufw allow "$frontend_port/tcp" comment 'Octo Frontend'
      fi

      # Enable UFW
      log_warn "Enabling UFW firewall. Make sure SSH port $ssh_port is correct!"
      echo "y" | sudo ufw enable

      sudo ufw status verbose
      log_success "UFW firewall configured"
    else
      log_warn "UFW not found, skipping firewall configuration"
    fi
    ;;
  fedora | centos | rhel | rocky | alma | opensuse* | suse*)
    if command_exists firewall-cmd; then
      log_info "Configuring firewalld..."

      sudo systemctl enable --now firewalld

      # Allow SSH
      sudo firewall-cmd --permanent --add-port="$ssh_port/tcp"

      # Allow HTTP/HTTPS for Caddy
      if [[ "$SETUP_CADDY" == "yes" ]]; then
        sudo firewall-cmd --permanent --add-service=http
        sudo firewall-cmd --permanent --add-service=https
      fi

      # Allow Octo ports (only if not using Caddy)
      if [[ "$SETUP_CADDY" != "yes" ]]; then
        sudo firewall-cmd --permanent --add-port="$octo_port/tcp"
        sudo firewall-cmd --permanent --add-port="$frontend_port/tcp"
      fi

      sudo firewall-cmd --reload
      sudo firewall-cmd --list-all
      log_success "firewalld configured"
    else
      log_warn "firewalld not found, skipping firewall configuration"
    fi
    ;;
  esac
}

# Configure fail2ban
configure_fail2ban() {
  if [[ "$OCTO_SETUP_FAIL2BAN" != "yes" ]]; then
    log_info "Skipping fail2ban configuration"
    return
  fi

  log_step "Configuring fail2ban"

  local ssh_port="${OCTO_SSH_PORT:-22}"
  local jail_local="/etc/fail2ban/jail.local"

  # Create jail.local configuration
  sudo tee "$jail_local" >/dev/null <<EOF
# Fail2ban Configuration for Octo Server
# Generated by setup.sh on $(date)

[DEFAULT]
# Ban duration: 1 hour
bantime = 3600

# Time window for counting failures: 10 minutes
findtime = 600

# Number of failures before ban
maxretry = 5

# Ignore localhost
ignoreip = 127.0.0.1/8 ::1

# Default action: ban with UFW/firewalld
banaction = ufw
banaction_allports = ufw

[sshd]
enabled = true
port = $ssh_port
filter = sshd
logpath = %(sshd_log)s
backend = systemd
maxretry = 3
bantime = 3600
findtime = 600
EOF

  # Use firewalld action on RHEL-based and openSUSE systems
  if [[ "$OS_DISTRO" =~ ^(fedora|centos|rhel|rocky|alma|opensuse|suse).*$ ]]; then
    sudo sed -i 's/banaction = ufw/banaction = firewallcmd-ipset/' "$jail_local"
    sudo sed -i 's/banaction_allports = ufw/banaction_allports = firewallcmd-ipset/' "$jail_local"
  fi

  # Enable and restart fail2ban
  sudo systemctl enable fail2ban
  sudo systemctl restart fail2ban

  # Show status
  sudo fail2ban-client status
  log_success "fail2ban configured"
}

# Harden SSH configuration
harden_ssh() {
  if [[ "$OCTO_HARDEN_SSH" != "yes" ]]; then
    log_info "Skipping SSH hardening"
    return
  fi

  log_step "Hardening SSH configuration"

  local ssh_port="${OCTO_SSH_PORT:-22}"
  local sshd_config_dir="/etc/ssh/sshd_config.d"
  local hardening_conf="$sshd_config_dir/00-octo-hardening.conf"

  # Ensure sshd_config.d directory exists
  sudo mkdir -p "$sshd_config_dir"

  # Check if Include directive exists in main config
  if ! sudo grep -q "^Include /etc/ssh/sshd_config.d/\*.conf" /etc/ssh/sshd_config; then
    log_info "Adding Include directive to sshd_config..."
    sudo sed -i '1i Include /etc/ssh/sshd_config.d/*.conf' /etc/ssh/sshd_config
  fi

  # Create hardening configuration
  log_info "Creating SSH hardening configuration..."
  sudo tee "$hardening_conf" >/dev/null <<EOF
# SSH Hardening Configuration for Octo Server
# Generated by setup.sh on $(date)
#
# This file applies security best practices for SSH.
# Edit with caution - incorrect settings can lock you out!

# Port Configuration
Port $ssh_port

# Strong Cryptography
KexAlgorithms curve25519-sha256,curve25519-sha256@libssh.org,diffie-hellman-group16-sha512,diffie-hellman-group18-sha512
Ciphers chacha20-poly1305@openssh.com,aes256-gcm@openssh.com,aes128-gcm@openssh.com,aes256-ctr
MACs hmac-sha2-512-etm@openssh.com,hmac-sha2-256-etm@openssh.com,hmac-sha2-512,hmac-sha2-256

# Enhanced Logging
LogLevel VERBOSE

# Authentication Hardening
LoginGraceTime 30s
PermitRootLogin no
StrictModes yes
MaxAuthTries 3
MaxSessions 10

# Force Public Key Authentication Only
PubkeyAuthentication yes
PasswordAuthentication no
PermitEmptyPasswords no

# Disable Challenge-Response (for TOTP, enable if using 2FA)
KbdInteractiveAuthentication no

# Disable Forwarding by Default
AllowAgentForwarding no
AllowTcpForwarding no
X11Forwarding no
PermitTunnel no

# Connection Timeouts
ClientAliveInterval 300
ClientAliveCountMax 2

# Restrict to admin user if set
EOF

  # Add AllowUsers if admin user is set
  if [[ -n "$ADMIN_USERNAME" ]]; then
    echo "AllowUsers $ADMIN_USERNAME" | sudo tee -a "$hardening_conf" >/dev/null
  fi

  # Validate configuration
  log_info "Validating SSH configuration..."
  if sudo sshd -t; then
    log_success "SSH configuration is valid"

    # Restart SSH
    local ssh_service="sshd"
    [[ "$OS_DISTRO" =~ ^(debian|ubuntu|pop|linuxmint)$ ]] && ssh_service="ssh"

    log_warn "Restarting SSH service. Make sure you can still connect!"
    sudo systemctl restart "$ssh_service"
    log_success "SSH hardening applied"
  else
    log_error "SSH configuration validation failed! Reverting..."
    sudo rm -f "$hardening_conf"
    return 1
  fi
}

# Configure automatic security updates
configure_auto_updates() {
  if [[ "$OCTO_SETUP_AUTO_UPDATES" != "yes" ]]; then
    log_info "Skipping automatic updates configuration"
    return
  fi

  log_step "Configuring automatic security updates"

  case "$OS_DISTRO" in
  debian | ubuntu | pop | linuxmint)
    # Configure unattended-upgrades
    sudo tee /etc/apt/apt.conf.d/50unattended-upgrades >/dev/null <<'EOF'
// Automatic security updates configuration
// Generated by Octo setup.sh

Unattended-Upgrade::Allowed-Origins {
    "${distro_id}:${distro_codename}-security";
    "${distro_id}ESMApps:${distro_codename}-apps-security";
    "${distro_id}ESM:${distro_codename}-infra-security";
};

Unattended-Upgrade::AutoFixInterruptedDpkg "true";
Unattended-Upgrade::MinimalSteps "true";
Unattended-Upgrade::Remove-Unused-Kernel-Packages "true";
Unattended-Upgrade::Remove-Unused-Dependencies "true";

// Don't auto-reboot (manual control is safer for servers)
Unattended-Upgrade::Automatic-Reboot "false";

// Log to syslog
Unattended-Upgrade::SyslogEnable "true";
EOF

    sudo tee /etc/apt/apt.conf.d/20auto-upgrades >/dev/null <<'EOF'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
APT::Periodic::Download-Upgradeable-Packages "1";
APT::Periodic::AutocleanInterval "7";
EOF

    log_success "Automatic security updates configured (Debian/Ubuntu)"
    ;;
  fedora | centos | rhel | rocky | alma)
    # Configure dnf-automatic
    sudo tee /etc/dnf/automatic.conf >/dev/null <<'EOF'
[commands]
# Only apply security updates automatically
upgrade_type = security
random_sleep = 360

# Download updates but don't apply automatically (safer)
download_updates = yes
apply_updates = no

[emitters]
system_name = octo-server
emit_via = stdio

[email]
email_from = root@localhost
email_to = root

[command]
[command_email]
[base]
EOF

    sudo systemctl enable --now dnf-automatic.timer
    log_success "Automatic security updates configured (dnf-automatic)"
    ;;
  arch | manjaro | endeavouros)
    log_warn "Arch Linux detected. Automatic updates are not recommended for Arch."
    log_info "Consider using 'pacman -Syu' manually or setting up pacman hooks."
    ;;
  opensuse* | suse*)
    # Configure transactional-update or zypper automatic patches
    log_info "Configuring automatic security updates for openSUSE..."
    if command_exists transactional-update; then
      # For MicroOS / transactional systems
      sudo systemctl enable --now transactional-update.timer
      log_success "Transactional updates enabled"
    else
      # For traditional openSUSE
      sudo zypper install -y zypper-lifecycle-plugin
      # Enable automatic security patches
      sudo tee /etc/zypp/zypp.conf.d/auto-updates.conf >/dev/null <<'EOCONF'
# Automatic security updates
solver.onlyRequires = true
EOCONF
      log_info "openSUSE: Run 'sudo zypper patch --category security' periodically"
      log_info "Consider setting up a cron job or systemd timer for automatic patches"
    fi
    ;;
  *)
    log_warn "Unknown distribution. Skipping automatic updates configuration."
    ;;
  esac
}

# Apply kernel security parameters
harden_kernel() {
  if [[ "$OCTO_HARDEN_KERNEL" != "yes" ]]; then
    log_info "Skipping kernel hardening"
    return
  fi

  log_step "Applying kernel security parameters"

  local sysctl_conf="/etc/sysctl.d/99-octo-hardening.conf"

  sudo tee "$sysctl_conf" >/dev/null <<'EOF'
# Kernel Security Parameters for Octo Server
# Generated by setup.sh

# Prevent IP spoofing
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.default.rp_filter = 1

# Disable ICMP redirect acceptance
net.ipv4.conf.all.accept_redirects = 0
net.ipv4.conf.default.accept_redirects = 0
net.ipv6.conf.all.accept_redirects = 0
net.ipv6.conf.default.accept_redirects = 0

# Disable ICMP redirect sending
net.ipv4.conf.all.send_redirects = 0
net.ipv4.conf.default.send_redirects = 0

# Disable source routing
net.ipv4.conf.all.accept_source_route = 0
net.ipv4.conf.default.accept_source_route = 0
net.ipv6.conf.all.accept_source_route = 0
net.ipv6.conf.default.accept_source_route = 0

# Enable TCP SYN cookies (protection against SYN flood attacks)
net.ipv4.tcp_syncookies = 1

# Ignore ICMP broadcast requests
net.ipv4.icmp_echo_ignore_broadcasts = 1

# Ignore bogus ICMP errors
net.ipv4.icmp_ignore_bogus_error_responses = 1

# Log martian packets (packets with impossible addresses)
net.ipv4.conf.all.log_martians = 1
net.ipv4.conf.default.log_martians = 1

# Disable IPv6 if not needed (uncomment if you don't use IPv6)
# net.ipv6.conf.all.disable_ipv6 = 1
# net.ipv6.conf.default.disable_ipv6 = 1

# Prevent core dumps from being written to disk (security)
fs.suid_dumpable = 0

# Restrict kernel pointer exposure
kernel.kptr_restrict = 2

# Restrict dmesg access to root
kernel.dmesg_restrict = 1
EOF

  # Apply sysctl settings
  sudo sysctl -p "$sysctl_conf"

  log_success "Kernel security parameters applied"
}

# Enable audit logging
enable_audit_logging() {
  log_step "Enabling audit logging"

  if command_exists auditd; then
    sudo systemctl enable auditd
    sudo systemctl start auditd
    log_success "Audit logging enabled"
  else
    log_warn "auditd not installed, skipping"
  fi
}

# Main hardening function
harden_server() {
  if ! should_harden_server; then
    log_info "Server hardening skipped"
    return
  fi

  log_step "Starting server hardening"

  # Check for root/sudo
  if [[ $EUID -ne 0 ]] && ! sudo -n true 2>/dev/null; then
    log_warn "Server hardening requires sudo privileges"
    if ! confirm "Continue with server hardening (will prompt for sudo password)?"; then
      log_info "Skipping server hardening"
      return
    fi
  fi

  # Warn about SSH changes
  echo
  log_warn "╔══════════════════════════════════════════════════════════════════╗"
  log_warn "║  WARNING: SSH hardening will disable password authentication!    ║"
  log_warn "║  Make sure you have SSH key access before continuing.            ║"
  log_warn "║  SSH port will be set to: ${OCTO_SSH_PORT:-22}                              ║"
  log_warn "╚══════════════════════════════════════════════════════════════════╝"
  echo

  if [[ "$NONINTERACTIVE" != "true" ]]; then
    if ! confirm "Continue with server hardening?" "n"; then
      log_info "Server hardening cancelled"
      return
    fi
  fi

  # Install security packages
  install_security_packages

  # Configure firewall
  configure_firewall

  # Configure fail2ban
  configure_fail2ban

  # Harden SSH
  harden_ssh

  # Configure automatic updates
  configure_auto_updates

  # Harden kernel
  harden_kernel

  # Enable audit logging
  enable_audit_logging

  log_success "Server hardening complete!"
  echo
  echo "Security summary:"
  echo "  - Firewall:        $(command_exists ufw && echo 'UFW' || (command_exists firewall-cmd && echo 'firewalld' || echo 'not configured'))"
  echo "  - Fail2ban:        $(systemctl is-active fail2ban 2>/dev/null || echo 'not running')"
  echo "  - SSH hardening:   ${OCTO_HARDEN_SSH}"
  echo "  - Auto updates:    ${OCTO_SETUP_AUTO_UPDATES}"
  echo "  - Kernel hardening: ${OCTO_HARDEN_KERNEL}"
  echo "  - Audit logging:   $(systemctl is-active auditd 2>/dev/null || echo 'not running')"
  echo
}

# ==============================================================================
# Service Installation
# ==============================================================================

# Ensure the octo system user exists with a proper home directory.
# Called early in multi-user setup so EAVS/hstry/mmry config can be
# written into ~octo/.config/ before services are installed.
# Safe to call multiple times (idempotent).
OCTO_HOME="/home/octo"

ensure_octo_system_user() {
  if id octo &>/dev/null; then
    OCTO_HOME=$(eval echo "~octo")
    return 0
  fi

  log_info "Creating octo system user with home at $OCTO_HOME..."
  # Use /bin/bash so admins can: sudo -su octo
  # No password set, so direct/SSH login is impossible.
  sudo useradd -r -m -d "$OCTO_HOME" -s /bin/bash octo

  # Create XDG directory structure
  sudo mkdir -p \
    "${OCTO_HOME}/.config" \
    "${OCTO_HOME}/.local/share" \
    "${OCTO_HOME}/.local/state"
  sudo chown -R octo:octo "$OCTO_HOME"

  log_success "Created octo system user (home: $OCTO_HOME)"
}

install_service_linux() {
  log_step "Installing systemd service"

  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    # User-level service
    local service_dir="$HOME/.config/systemd/user"
    mkdir -p "$service_dir"

    local service_file="$service_dir/octo.service"

    cat >"$service_file" <<EOF
# Octo Server - User service
# Generated by setup.sh

[Unit]
Description=Octo Server (User Mode)
After=default.target

[Service]
Type=simple
Environment=OCTO_CONFIG=$OCTO_CONFIG_DIR/config.toml
Environment=RUST_LOG=$OCTO_LOG_LEVEL
EnvironmentFile=-$OCTO_CONFIG_DIR/env
ExecStart=/usr/local/bin/octo serve --local-mode
ExecStop=/bin/kill -TERM \$MAINPID
TimeoutStopSec=30
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    log_success "Service file created: $service_file"

    if confirm "Enable and start the service now?"; then
      systemctl --user daemon-reload
      systemctl --user enable octo
      systemctl --user start octo
      log_success "Service enabled and started"
      log_info "Check status with: systemctl --user status octo"
      log_info "View logs with: journalctl --user -u octo -f"
    else
      log_info "To enable manually:"
      log_info "  systemctl --user daemon-reload"
      log_info "  systemctl --user enable --now octo"
    fi
  else
    # System-level service (requires sudo)
    log_info "Multi-user mode requires system-level service installation"

    if ! confirm "Install system service? (requires sudo)"; then
      log_info "Skipping service installation"
      return
    fi

    # Ensure octo user exists (may already be created by ensure_octo_system_user)
    ensure_octo_system_user
    local octo_home="$OCTO_HOME"

    # Create runtime directories
    sudo mkdir -p /run/octo
    sudo chown octo:octo /run/octo

    # Runtime config in octo's home (XDG layout: ~/.config/octo/)
    # This is what the octo service actually reads at startup.
    local octo_config_home="${octo_home}/.config/octo"
    sudo mkdir -p "$octo_config_home"
    sudo cp "$OCTO_CONFIG_DIR/config.toml" "${octo_config_home}/config.toml"
    if [[ -f "$OCTO_CONFIG_DIR/env" ]]; then
      sudo cp "$OCTO_CONFIG_DIR/env" "${octo_config_home}/env"
      sudo chmod 600 "${octo_config_home}/env"
    fi

    # Also copy a baseline config to /etc/octo/ for sandbox policy reference.
    # Sandbox configs (sandbox.toml, skdlr-agent.toml) live here too - these
    # are system-wide policy the admin controls, not per-service runtime config.
    sudo mkdir -p /etc/octo
    sudo cp "$OCTO_CONFIG_DIR/config.toml" /etc/octo/config.toml

    sudo chown -R octo:octo "$octo_home"

    # Install service file
    local service_file="/etc/systemd/system/octo.service"

    sudo tee "$service_file" >/dev/null <<EOF
# Octo Server - System service
# Generated by setup.sh

[Unit]
Description=Octo Control Plane Server
After=network.target eavs.service
Wants=eavs.service

[Service]
Type=simple
User=octo
Group=octo
WorkingDirectory=${octo_home}
Environment=HOME=${octo_home}
Environment=XDG_CONFIG_HOME=${octo_home}/.config
Environment=XDG_DATA_HOME=${octo_home}/.local/share
Environment=XDG_STATE_HOME=${octo_home}/.local/state
Environment=OCTO_CONFIG=${octo_config_home}/config.toml
Environment=RUST_LOG=$OCTO_LOG_LEVEL
EnvironmentFile=-${octo_config_home}/env
RuntimeDirectory=octo
RuntimeDirectoryMode=0755
ExecStart=/usr/local/bin/octo serve
ExecStop=/bin/kill -TERM \$MAINPID
TimeoutStopSec=30
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=${octo_home}
ReadWritePaths=/run/octo
PrivateTmp=true
AmbientCapabilities=CAP_NET_BIND_SERVICE

[Install]
WantedBy=multi-user.target
EOF

    # Binaries are already in /usr/local/bin from build_octo

    log_success "Service file created: $service_file"

    # Install octo-runner user service template for multi-user mode
    install_runner_service

    if confirm "Enable and start the service now?"; then
      sudo systemctl daemon-reload
      sudo systemctl enable octo
      sudo systemctl start octo
      log_success "Service enabled and started"
      log_info "Check status with: sudo systemctl status octo"
      log_info "View logs with: sudo journalctl -u octo -f"
    fi
  fi
}

install_runner_service() {
  # Install octo-runner as a systemd user service template
  # Each user runs their own instance of octo-runner for process isolation
  log_info "Installing octo-runner user service template..."

  local runner_service="/etc/systemd/user/octo-runner.service"
  local tmpfiles_conf="/etc/tmpfiles.d/octo-runner.conf"

  # Service file
  sudo tee "$runner_service" >/dev/null <<'EOF'
# Octo Runner - Per-user process runner for multi-user isolation
# This service runs as the logged-in user and manages their agent processes

[Unit]
Description=Octo Runner (User Process Manager)
After=default.target

[Service]
Type=simple
ExecStart=/usr/local/bin/octo-runner --socket /run/octo/runner-sockets/%u/octo-runner.sock
ExecStop=/bin/kill -TERM $MAINPID
TimeoutStopSec=30
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

  # Ensure shared runner socket base exists at boot.
  sudo tee "$tmpfiles_conf" >/dev/null <<'EOF'
d /run/octo/runner-sockets 2770 root octo -
EOF

  sudo systemd-tmpfiles --create "$tmpfiles_conf" >/dev/null 2>&1 || true

  # Ensure shared group exists and current user can connect to runner sockets.
  # (Group membership changes require re-login to take effect.)
  sudo groupadd -f octo >/dev/null 2>&1 || true
  sudo usermod -a -G octo "${SUDO_USER:-$USER}" >/dev/null 2>&1 || true

  # Ensure shared socket directory exists for current user.
  sudo install -d -m 2770 -o "${SUDO_USER:-$USER}" -g octo "/run/octo/runner-sockets/${SUDO_USER:-$USER}" >/dev/null 2>&1 || true

  log_success "octo-runner service template installed"
  log_info "Users can enable it with: systemctl --user enable --now octo-runner"
}

install_service_macos() {
  log_step "Installing launchd service"

  local plist_dir="$HOME/Library/LaunchAgents"
  local log_dir="$HOME/Library/Logs"
  mkdir -p "$plist_dir" "$log_dir"

  local plist_file="$plist_dir/ai.octo.server.plist"

  # Determine serve flags
  local serve_flags=""
  if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
    serve_flags="<string>--local-mode</string>"
  fi

  cat >"$plist_file" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>ai.octo.server</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/octo</string>
        <string>serve</string>
        $serve_flags
    </array>

    <key>EnvironmentVariables</key>
    <dict>
        <key>OCTO_CONFIG</key>
        <string>$OCTO_CONFIG_DIR/config.toml</string>
        <key>RUST_LOG</key>
        <string>$OCTO_LOG_LEVEL</string>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:$HOME/.bun/bin</string>
    </dict>

    <key>WorkingDirectory</key>
    <string>$HOME</string>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>

    <key>ThrottleInterval</key>
    <integer>5</integer>

    <key>StandardOutPath</key>
    <string>$log_dir/octo.stdout.log</string>

    <key>StandardErrorPath</key>
    <string>$log_dir/octo.stderr.log</string>
</dict>
</plist>
EOF

  log_success "Launchd plist created: $plist_file"

  if confirm "Load and start the service now?"; then
    # Unload if already loaded
    launchctl unload "$plist_file" 2>/dev/null || true
    launchctl load "$plist_file"
    log_success "Service loaded and started"
    log_info "Check status with: launchctl list | grep octo"
    log_info "View logs at: $log_dir/octo.*.log"
  else
    log_info "To load manually:"
    log_info "  launchctl load $plist_file"
  fi
}

install_service() {
  if [[ "$OCTO_INSTALL_SERVICE" != "yes" ]]; then
    log_info "Skipping service installation (OCTO_INSTALL_SERVICE=no)"
    return
  fi

  case "$OS" in
  linux)
    install_service_linux
    ;;
  macos)
    install_service_macos
    ;;
  esac
}

# ==============================================================================
# Container Image Build
# ==============================================================================

build_container_image() {
  if [[ "$SELECTED_BACKEND_MODE" != "container" ]]; then
    return
  fi

  log_step "Building container image"

  if ! confirm "Build the Octo container image? (this may take several minutes)"; then
    log_info "Skipping container build"
    log_info "You can build later with: just container-build"
    return
  fi

  cd "$SCRIPT_DIR"

  local dockerfile="container/Dockerfile"
  if [[ "$ARCH" == "arm64" || "$ARCH" == "aarch64" ]]; then
    if [[ -f "container/Dockerfile.arm64" ]]; then
      dockerfile="container/Dockerfile.arm64"
    fi
  fi

  log_info "Building image with $CONTAINER_RUNTIME..."
  $CONTAINER_RUNTIME build -t octo-dev:latest -f "$dockerfile" .

  log_success "Container image built: octo-dev:latest"
}

# ==============================================================================
# Summary and Next Steps
# ==============================================================================

print_summary() {
  log_step "Setup Complete!"

  echo
  echo "============================================================"
  echo "                    SERVICE STATUS"
  echo "============================================================"
  echo

  # Helper to check service status
  check_service_status() {
    local name="$1"
    local user_service="${2:-false}"

    if [[ "$user_service" == "true" ]]; then
      if systemctl --user is-active "$name" &>/dev/null; then
        echo -e "${GREEN}running${NC}"
      elif systemctl --user is-enabled "$name" &>/dev/null; then
        echo -e "${YELLOW}enabled (not running)${NC}"
      else
        echo -e "${RED}not configured${NC}"
      fi
    else
      if systemctl is-active "$name" &>/dev/null; then
        echo -e "${GREEN}running${NC}"
      elif systemctl is-enabled "$name" &>/dev/null; then
        echo -e "${YELLOW}enabled (not running)${NC}"
      else
        echo -e "${RED}not configured${NC}"
      fi
    fi
  }

  local is_user_service="false"
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    is_user_service="true"
  fi

  echo -e "  EAVS (LLM):     $(check_service_status eavs "$is_user_service")"
  echo -e "  Octo backend:   $(check_service_status octo "$is_user_service")"

  if [[ "$SETUP_CADDY" == "yes" ]]; then
    echo -e "  Caddy:          $(check_service_status caddy)"
  fi

  echo -e "  SearXNG:        $(check_service_status searxng "$is_user_service")"

  if command_exists valkey-server; then
    echo -e "  Valkey:         $(check_service_status valkey)"
  elif command_exists redis-server; then
    echo -e "  Redis:          $(check_service_status redis)"
  fi

  if [[ "$OS" == "linux" ]]; then
    echo -e "  hstry:          $(check_service_status hstry "$is_user_service")"
  fi

  echo
  echo "============================================================"
  echo "                    CONFIGURATION"
  echo "============================================================"
  echo
  echo "  User mode:       $SELECTED_USER_MODE"
  echo "  Backend mode:    $SELECTED_BACKEND_MODE"
  echo "  Deployment mode: $([[ "$PRODUCTION_MODE" == "true" ]] && echo "Production" || echo "Development")"
  echo "  Config file:     $OCTO_CONFIG_DIR/config.toml"
  echo

  if [[ "$PRODUCTION_MODE" == "true" ]]; then
    echo "  Security:"
    echo "    JWT secret:    configured (64 characters)"
    echo "    Admin user:    $ADMIN_USERNAME"
    echo "    Admin email:   $ADMIN_EMAIL"
    if [[ "$NONINTERACTIVE" == "true" ]]; then
      echo -e "    ${YELLOW}Admin password: $ADMIN_PASSWORD${NC}"
      echo -e "    ${RED}SAVE THIS PASSWORD - it will not be shown again!${NC}"
    fi
    echo

    if [[ "$SETUP_CADDY" == "yes" ]]; then
      echo "  Reverse Proxy:"
      echo "    Caddy:         installed"
      echo "    Domain:        $DOMAIN"
      if [[ "$DOMAIN" != "localhost" ]]; then
        echo "    HTTPS:         enabled (automatic via Let's Encrypt)"
      fi
      echo "    Caddyfile:     /etc/caddy/Caddyfile"
      echo
    fi

    if [[ "$OCTO_HARDEN_SERVER" == "yes" && "$OS" == "linux" ]]; then
      echo "  Server Hardening:"
      echo "    Firewall:      $(command_exists ufw && echo 'UFW enabled' || (command_exists firewall-cmd && echo 'firewalld enabled' || echo 'not configured'))"
      echo -e "    Fail2ban:      $(check_service_status fail2ban)"
      echo "    SSH port:      ${OCTO_SSH_PORT:-22}"
      echo "    SSH auth:      public key only (password disabled)"
      echo "    Auto updates:  ${OCTO_SETUP_AUTO_UPDATES}"
      echo "    Kernel:        hardened sysctl parameters"
      echo -e "    Audit:         $(check_service_status auditd)"
      echo
    fi
  fi

  local eavs_cfg
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_cfg="${XDG_CONFIG_HOME}/eavs/config.toml"
  else
    eavs_cfg="/etc/eavs/config.toml"
  fi

  echo "  LLM Access (EAVS):"
  echo "    Proxy URL:     http://127.0.0.1:${EAVS_PORT}"
  echo "    Config:        $eavs_cfg"
  if [[ -f "$eavs_cfg" ]]; then
    local configured_providers
    configured_providers=$(grep '^\[providers\.' "$eavs_cfg" 2>/dev/null | sed 's/\[providers\.\(.*\)\]/\1/' | grep -v '^default$' | tr '\n' ', ' | sed 's/,$//')
    if [[ -n "$configured_providers" ]]; then
      echo -e "    Providers:     ${GREEN}${configured_providers}${NC}"
    else
      echo -e "    Providers:     ${RED}none configured${NC}"
    fi
  else
    echo -e "    Providers:     ${YELLOW}config not found${NC}"
  fi

  echo
  echo "============================================================"
  echo "                    INSTALLED SOFTWARE"
  echo "============================================================"
  echo

  # Helper: check if binary exists and show path or red "missing"
  check_bin() {
    local name="$1"
    local path
    path=$(which "$name" 2>/dev/null)
    if [[ -n "$path" ]]; then
      echo -e "${GREEN}$path${NC}"
    else
      echo -e "${RED}missing${NC}"
    fi
  }

  echo "  Core binaries:"
  echo -e "    octo:          $(check_bin octo)"
  echo -e "    eavs:          $(check_bin eavs)"
  echo -e "    fileserver:    $(check_bin fileserver)"
  echo -e "    pi:            $(check_bin pi)"
  if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
    echo -e "    ttyd:          $(check_bin ttyd)"
  fi
  if [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
    echo -e "    octo-runner:   $(check_bin octo-runner)"
  fi
  if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
    echo -e "    pi-bridge:     $(check_bin pi-bridge)"
  fi
  echo

  echo "  Agent tools:"
  for tool in agntz mmry scrpr sx tmpltr sldr ignr; do
    printf "    %-14s " "$tool:"
    echo -e "$(check_bin "$tool")"
  done
  echo

  echo "  Shell tools:"
  for tool in tmux fd rg yazi zsh zoxide; do
    printf "    %-14s " "$tool:"
    echo -e "$(check_bin "$tool")"
  done
  echo

  echo "  Pi extensions:"
  local pi_ext_dir="$HOME/.pi/agent/extensions"
  for ext_name in "${PI_DEFAULT_EXTENSIONS[@]}"; do
    printf "    %-22s " "${ext_name}:"
    if [[ -d "${pi_ext_dir}/${ext_name}" ]]; then
      echo -e "${GREEN}installed${NC}"
    else
      echo -e "${RED}missing${NC}"
    fi
  done
  echo

  echo "============================================================"
  echo "                    NEXT STEPS"
  echo "============================================================"
  echo

  local step=1

  # Check which services need starting
  local need_start=()

  # Helper: check if a service needs starting
  service_needs_start() {
    local svc="$1"
    if [[ "$SELECTED_USER_MODE" == "single" ]]; then
      ! systemctl --user is-active "$svc" &>/dev/null
    else
      ! systemctl is-active "$svc" &>/dev/null
    fi
  }

  if [[ "$OS" == "linux" ]]; then
    service_needs_start eavs && need_start+=("eavs")
    service_needs_start octo && need_start+=("octo")
    if [[ "$SETUP_CADDY" == "yes" ]]; then
      service_needs_start caddy && need_start+=("caddy")
    fi
    if systemctl --user is-enabled searxng &>/dev/null || systemctl is-enabled searxng &>/dev/null; then
      service_needs_start searxng && need_start+=("searxng")
    fi
  fi

  if [[ ${#need_start[@]} -gt 0 ]]; then
    echo "  $step. Start services that are not yet running:"
    for svc in "${need_start[@]}"; do
      if [[ "$SELECTED_USER_MODE" == "single" ]]; then
        echo "     systemctl --user start $svc"
      else
        echo "     sudo systemctl start $svc"
      fi
    done
    echo
    ((step++))
  fi

  if [[ "$PRODUCTION_MODE" == "true" ]]; then
    echo "  $step. Start the frontend (production build):"
    echo "     cd $SCRIPT_DIR/frontend && bun run preview"
    echo "     # Or deploy the dist/ folder to your web server"
    echo
    ((step++))

    echo "  $step. Access the web interface:"
    if [[ -n "$DOMAIN" && "$DOMAIN" != "localhost" ]]; then
      echo "     https://${DOMAIN}"
    else
      echo "     http://localhost:3000"
    fi
    echo
    ((step++))

    echo "  $step. Login with admin credentials:"
    echo "     Username: $ADMIN_USERNAME"
    echo "     Password: (the password you entered during setup)"
    echo
    ((step++))

    echo "  $step. Create invite codes for new users:"
    echo "     octoctl invites create --uses 1"
    echo "     # Or use the admin interface"
    echo
  else
    echo "  $step. Start the frontend dev server:"
    echo "     cd $SCRIPT_DIR/frontend && bun dev"
    echo
    ((step++))

    echo "  $step. Open the web interface:"
    echo "     http://localhost:3000"
    echo
    ((step++))

    if [[ "$OCTO_DEV_MODE" == "true" && -n "${dev_user_id:-}" ]]; then
      echo "  $step. Login with your dev credentials:"
      echo "     Username: $dev_user_id"
      echo "     Password: (the password you entered)"
      echo
      ((step++))
    fi
  fi

  # Show API key warning if not configured
  if [[ "$EAVS_ENABLED" != "true" && "$LLM_API_KEY_SET" != "true" && -n "$LLM_PROVIDER" ]]; then
    echo -e "  ${YELLOW}IMPORTANT:${NC} Set your API key before starting Octo:"
    case "$LLM_PROVIDER" in
    anthropic)
      echo "     export ANTHROPIC_API_KEY=your-key-here"
      ;;
    openai)
      echo "     export OPENAI_API_KEY=your-key-here"
      ;;
    openrouter)
      echo "     export OPENROUTER_API_KEY=your-key-here"
      ;;
    google)
      echo "     export GOOGLE_API_KEY=your-key-here"
      ;;
    groq)
      echo "     export GROQ_API_KEY=your-key-here"
      ;;
    esac
    echo
  fi

  # macOS note about env file
  if [[ "$OS" == "macos" && "$LLM_API_KEY_SET" == "true" ]]; then
    echo "  Note: On macOS, source the env file before starting manually:"
    echo "     source $OCTO_CONFIG_DIR/env"
    echo
  fi

  echo "For more information, see:"
  echo "  - README.md"
  echo "  - SETUP.md (detailed setup guide)"
  echo "  - deploy/systemd/README.md (Linux systemd setup)"
  echo "  - deploy/ansible/README.md (Ansible deployment)"
  echo "  - backend/examples/config.toml (full config reference)"
}

# ==============================================================================
# Main
# ==============================================================================

show_help() {
  cat <<EOF
Octo Setup Script

Usage: $0 [OPTIONS]

Options:
  --help                Show this help message
  --non-interactive     Run without prompts (uses defaults/env vars)
  
  --production, --prod  Production mode with ALL hardening enabled:
                        - Disables dev mode (requires real auth)
                        - Enables firewall, fail2ban, SSH hardening
                        - Enables auto-updates and kernel hardening
                        - Installs all dependencies and services
  
  --dev, --development  Development mode (no hardening, dev auth enabled)
  
  --domain <domain>     Set domain and enable Caddy reverse proxy
                        Example: --domain octo.example.com
  
  --ssh-port <port>     Set SSH port for hardening (default: 22)
  
  Disable specific hardening features (use with --production):
  --no-firewall         Skip firewall configuration
  --no-fail2ban         Skip fail2ban installation
  --no-ssh-hardening    Skip SSH hardening (keeps password auth)
  --no-auto-updates     Skip automatic security updates
  --no-kernel-hardening Skip kernel sysctl hardening
  
  Tool installation:
  --all-tools           Install all byteowlz agent tools
  --no-agent-tools      Skip agent tools installation

Environment Variables:
  OCTO_USER_MODE          single or multi (default: single)
  OCTO_BACKEND_MODE       local or container (default: local)
  OCTO_CONTAINER_RUNTIME  docker, podman, or auto (default: auto)
  OCTO_INSTALL_DEPS       yes or no (default: yes)
  OCTO_INSTALL_SERVICE    yes or no (default: yes)
  OCTO_INSTALL_AGENT_TOOLS yes or no (default: yes)
  OCTO_DEV_MODE           true or false (default: prompt user)
  OCTO_LOG_LEVEL          error, warn, info, debug, trace (default: info)
  OCTO_SETUP_CADDY        yes or no (default: prompt user in production mode)
  OCTO_DOMAIN             domain for HTTPS (e.g., octo.example.com)

Server Hardening (Linux production mode only):
  OCTO_HARDEN_SERVER      yes or no (default: prompt in production mode)
  OCTO_SSH_PORT           SSH port number (default: 22)
  OCTO_SETUP_FIREWALL     yes or no - configure UFW/firewalld (default: yes)
  OCTO_SETUP_FAIL2BAN     yes or no - install and configure fail2ban (default: yes)
  OCTO_HARDEN_SSH         yes or no - apply SSH hardening (default: yes)
  OCTO_SETUP_AUTO_UPDATES yes or no - enable automatic security updates (default: yes)
  OCTO_HARDEN_KERNEL      yes or no - apply kernel security parameters (default: yes)

LLM Provider API Keys (set one of these, or use EAVS):
  ANTHROPIC_API_KEY       Anthropic Claude API key
  OPENAI_API_KEY          OpenAI API key
  OPENROUTER_API_KEY      OpenRouter API key
  GOOGLE_API_KEY          Google AI API key
  GROQ_API_KEY            Groq API key

Shell Tools Installed:
  tmux, fd, ripgrep, yazi, zsh, zoxide

Agent Tools:
  agntz   - Agent toolkit (file reservations, tool management)
  mmry    - Memory storage and semantic search
  scrpr   - Web content extraction (readability, Tavily, Jina)
  sx      - Web search via local SearXNG instance
  tmpltr  - Document generation from templates (Typst)
  sldr    - Markdown presentations (Slidev)
  ignr    - Gitignore generation (auto-detect)

Other Tools:
  ttyd    - Web terminal
  pi      - Main chat interface (primary agent harness)

Search Engine:
  SearXNG - Local privacy-respecting metasearch engine (for sx)
  Valkey  - In-memory cache for SearXNG rate limiting

Pi Extensions (from github.com/byteowlz/pi-agent-extensions):
  auto-rename          - Auto-generate session names from first query
  octo-bridge          - Emit agent phase status for the Octo runner
  octo-todos           - Todo management for Octo UI
  custom-context-files - Auto-load USER.md, PERSONALITY.md into prompts

For detailed documentation on all prerequisites and components, see SETUP.md

Examples:
  # Interactive setup (recommended for first-time)
  ./setup.sh

  # Quick development setup (no prompts)
  ./setup.sh --dev

  # Full production setup with all hardening (RECOMMENDED for servers)
  ./setup.sh --production --domain octo.example.com

  # Production with custom SSH port
  ./setup.sh --production --domain octo.example.com --ssh-port 2222

  # Production but keep password SSH auth (for initial setup)
  ./setup.sh --production --domain octo.example.com --no-ssh-hardening

  # Multi-user container setup on Linux
  OCTO_USER_MODE=multi OCTO_BACKEND_MODE=container ./setup.sh --production

  # Environment variable style (equivalent to --production)
  OCTO_DEV_MODE=false OCTO_HARDEN_SERVER=yes ./setup.sh --non-interactive
EOF
}

main() {
  NONINTERACTIVE="false"

  # Parse arguments
  while [[ $# -gt 0 ]]; do
    case "$1" in
    --help | -h)
      show_help
      exit 0
      ;;
    --non-interactive)
      NONINTERACTIVE="true"
      shift
      ;;
    --production | --prod)
      # Production mode with all hardening enabled
      NONINTERACTIVE="true"
      OCTO_DEV_MODE="false"
      OCTO_HARDEN_SERVER="yes"
      OCTO_SETUP_FIREWALL="yes"
      OCTO_SETUP_FAIL2BAN="yes"
      OCTO_HARDEN_SSH="yes"
      OCTO_SETUP_AUTO_UPDATES="yes"
      OCTO_HARDEN_KERNEL="yes"
      OCTO_INSTALL_DEPS="yes"
      OCTO_INSTALL_SERVICE="yes"
      OCTO_INSTALL_AGENT_TOOLS="yes"
      shift
      ;;
    --dev | --development)
      # Development mode, no hardening
      NONINTERACTIVE="true"
      OCTO_DEV_MODE="true"
      OCTO_HARDEN_SERVER="no"
      shift
      ;;
    --domain)
      OCTO_DOMAIN="$2"
      OCTO_SETUP_CADDY="yes"
      shift 2
      ;;
    --domain=*)
      OCTO_DOMAIN="${1#*=}"
      OCTO_SETUP_CADDY="yes"
      shift
      ;;
    --ssh-port)
      OCTO_SSH_PORT="$2"
      shift 2
      ;;
    --ssh-port=*)
      OCTO_SSH_PORT="${1#*=}"
      shift
      ;;
    --no-firewall)
      OCTO_SETUP_FIREWALL="no"
      shift
      ;;
    --no-fail2ban)
      OCTO_SETUP_FAIL2BAN="no"
      shift
      ;;
    --no-ssh-hardening)
      OCTO_HARDEN_SSH="no"
      shift
      ;;
    --no-auto-updates)
      OCTO_SETUP_AUTO_UPDATES="no"
      shift
      ;;
    --no-kernel-hardening)
      OCTO_HARDEN_KERNEL="no"
      shift
      ;;
    --all-tools)
      INSTALL_ALL_TOOLS="true"
      INSTALL_MMRY="true"
      shift
      ;;
    --no-agent-tools)
      OCTO_INSTALL_AGENT_TOOLS="no"
      shift
      ;;
    *)
      log_error "Unknown option: $1"
      show_help
      exit 1
      ;;
    esac
  done

  echo
  echo -e "${BOLD}${CYAN}"
  echo "  ____       _        "
  echo " / __ \  ___| |_ ___  "
  echo "| |  | |/ __| __/ _ \ "
  echo "| |__| | (__| || (_) |"
  echo " \____/ \___|\__\___/ "
  echo -e "${NC}"
  echo -e "${BOLD}AI Agent Workspace Platform${NC}"
  echo

  # Initialize
  detect_os

  # Mode selection
  SELECTED_USER_MODE="${OCTO_USER_MODE}"
  SELECTED_BACKEND_MODE="${OCTO_BACKEND_MODE}"

  if [[ "$NONINTERACTIVE" != "true" ]]; then
    select_user_mode
    select_backend_mode
    select_deployment_mode
  else
    # Non-interactive: use env var or default to dev mode
    if [[ -z "$OCTO_DEV_MODE" ]]; then
      OCTO_DEV_MODE="true"
    fi
    PRODUCTION_MODE="$([[ "$OCTO_DEV_MODE" == "false" ]] && echo "true" || echo "false")"
  fi

  # Prerequisites
  check_prerequisites

  # Install dependencies
  if [[ "$OCTO_INSTALL_DEPS" == "yes" ]]; then
    # Shell tools (always useful)
    install_shell_tools

    if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
      install_ttyd
    fi

    # Pi extensions (always install for main chat integration)
    if [[ "$SELECTED_USER_MODE" == "multi" ]]; then
      install_pi_extensions_all_users
    else
      install_pi_extensions
    fi

    # Agent tools (agntz, mmry, scrpr, sx, tmpltr, sldr, ignr)
    if [[ "$OCTO_INSTALL_AGENT_TOOLS" == "yes" ]]; then
      install_agntz

      if [[ "$NONINTERACTIVE" != "true" ]]; then
        select_agent_tools
      fi

      if [[ "$INSTALL_MMRY" == "true" || "$INSTALL_ALL_TOOLS" == "true" ]]; then
        install_agent_tools_selected
      fi

      # Install SearXNG if sx was installed (all tools or explicitly)
      if [[ "$INSTALL_ALL_TOOLS" == "true" ]] || command_exists sx; then
        if confirm "Install SearXNG local search engine for sx?"; then
          install_searxng
        fi
      fi
    fi
  fi

  # In multi-user mode, create the octo system user early so that EAVS
  # and other tools can write config into its home directory.
  if [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
    ensure_octo_system_user
  fi

  # Install and configure EAVS (LLM proxy - mandatory for agent access)
  install_eavs
  configure_eavs
  install_eavs_service

  # Build Octo
  if confirm "Build Octo from source?"; then
    build_octo
  fi

  # Generate configuration
  generate_config

  # Setup onboarding templates repository and update shared external repos
  setup_onboarding_templates_repo
  update_external_repos
  setup_feedback_dirs

  # Setup Linux user isolation (if enabled)
  setup_linux_user_isolation

  # Build container image (if container mode)
  build_container_image

  # Install Caddy (if production mode)
  if [[ "$SETUP_CADDY" == "yes" ]]; then
    install_caddy
    generate_caddyfile
    install_caddy_service
  fi

  # Server hardening (Linux production only)
  harden_server

  # Install service
  install_service

  # Create admin user in database (production mode)
  create_admin_user_db

  # Summary
  print_summary
}

main "$@"
