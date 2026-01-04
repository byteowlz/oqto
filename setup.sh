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

# Default values (can be overridden by environment variables)
: "${OCTO_USER_MODE:=single}"           # single or multi
: "${OCTO_BACKEND_MODE:=local}"         # local or container
: "${OCTO_CONTAINER_RUNTIME:=auto}"     # docker, podman, or auto
: "${OCTO_INSTALL_DEPS:=yes}"           # yes or no
: "${OCTO_INSTALL_SERVICE:=yes}"        # yes or no
: "${OCTO_INSTALL_AGENT_TOOLS:=yes}"    # yes or no (mmry, trx, mailz via agntz)
: "${OCTO_DEV_MODE:=true}"              # true or false (auth dev mode)
: "${OCTO_LOG_LEVEL:=info}"             # error, warn, info, debug, trace

# Agent tools installation tracking
INSTALL_MMRY="false"
INSTALL_TRX="false"
INSTALL_MAILZ="false"

# Paths (XDG compliant)
: "${XDG_CONFIG_HOME:=$HOME/.config}"
: "${XDG_DATA_HOME:=$HOME/.local/share}"
: "${XDG_STATE_HOME:=$HOME/.local/state}"

OCTO_CONFIG_DIR="${XDG_CONFIG_HOME}/octo"
OCTO_DATA_DIR="${XDG_DATA_HOME}/octo"
OPENCODE_CONFIG_DIR="${XDG_CONFIG_HOME}/opencode"

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
    
    echo -e "\n${BOLD}$prompt${NC}"
    local i=1
    for opt in "${options[@]}"; do
        if [[ $i -eq 1 ]]; then
            echo "  $i) $opt (default)"
        else
            echo "  $i) $opt"
        fi
        ((i++))
    done
    
    local choice
    read -r -p "Enter choice [1-${#options[@]}]: " choice
    choice="${choice:-1}"
    
    if [[ "$choice" =~ ^[0-9]+$ ]] && [[ "$choice" -ge 1 ]] && [[ "$choice" -le "${#options[@]}" ]]; then
        echo "${options[$((choice-1))]}"
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

install_opencode() {
    log_step "Installing OpenCode"
    
    if command_exists opencode; then
        log_success "OpenCode already installed: $(opencode --version 2>/dev/null || echo 'version unknown')"
        if ! confirm "Reinstall OpenCode?"; then
            return 0
        fi
    fi
    
    log_info "Installing OpenCode..."
    curl -fsSL https://opencode.ai/install | bash
    
    if command_exists opencode; then
        log_success "OpenCode installed successfully"
    else
        log_warn "OpenCode installed but not in PATH. You may need to restart your shell."
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
                arch|manjaro)
                    log_info "Installing ttyd via pacman..."
                    sudo pacman -S --noconfirm ttyd
                    ;;
                debian|ubuntu)
                    log_info "Installing ttyd via apt..."
                    sudo apt-get update && sudo apt-get install -y ttyd
                    ;;
                fedora)
                    log_info "Installing ttyd via dnf..."
                    sudo dnf install -y ttyd
                    ;;
                *)
                    log_warn "Unknown distribution. Attempting to build from source..."
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
        arch|manjaro|endeavouros)
            install_shell_tools_arch "${tools[@]}"
            ;;
        debian|ubuntu|pop|linuxmint)
            install_shell_tools_debian "${tools[@]}"
            ;;
        fedora|rhel|centos|rocky|almalinux)
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
        sudo apt-get update
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
    
    for tool in "${tools[@]}"; do
        case "$tool" in
            yazi|yazi-fm)
                log_info "Installing yazi via cargo..."
                cargo install --locked yazi-fm yazi-cli
                ;;
            zoxide)
                log_info "Installing zoxide via cargo..."
                cargo install zoxide --locked
                ;;
            fd)
                log_info "Installing fd via cargo..."
                cargo install fd-find
                ;;
            ripgrep)
                log_info "Installing ripgrep via cargo..."
                cargo install ripgrep
                ;;
        esac
    done
}

# ==============================================================================
# Agent Tools Installation (agntz, mmry, trx, mailz)
# ==============================================================================

install_agntz() {
    log_step "Installing agntz (Agent Tools)"
    
    if command_exists agntz; then
        log_success "agntz already installed: $(agntz --version 2>/dev/null || echo 'version unknown')"
        if ! confirm "Reinstall agntz?"; then
            return 0
        fi
    fi
    
    if ! command_exists cargo; then
        log_error "Cargo not available. Cannot install agntz."
        return 1
    fi
    
    log_info "Installing agntz via cargo..."
    # agntz is part of the byteowlz tooling - install from crates.io or git
    # Assuming it's published to crates.io, otherwise use git install
    if cargo install agntz 2>/dev/null; then
        log_success "agntz installed via crates.io"
    else
        log_info "Trying to install from git repository..."
        cargo install --git https://github.com/byteowlz/agntz.git
    fi
    
    if command_exists agntz; then
        log_success "agntz installed successfully"
    else
        log_warn "agntz installation may have failed"
        return 1
    fi
}

select_agent_tools() {
    log_step "Agent Tools Selection"
    
    echo
    echo "Octo can install additional agent tools via agntz:"
    echo
    echo "  ${BOLD}mmry${NC} - Memory system for AI agents"
    echo "    - Persistent memory storage and retrieval"
    echo "    - Semantic search across memories"
    echo
    echo "  ${BOLD}trx${NC} - Transaction/task tracking"
    echo "    - Track agent operations"
    echo "    - Audit trail for actions"
    echo
    echo "  ${BOLD}mailz${NC} - Agent messaging system"
    echo "    - Cross-agent communication"
    echo "    - File reservation and coordination"
    echo
    
    if confirm "Install mmry (memory system)?"; then
        INSTALL_MMRY="true"
    fi
    
    if confirm "Install trx (transaction tracking)?"; then
        INSTALL_TRX="true"
    fi
    
    if confirm "Install mailz (agent messaging)?"; then
        INSTALL_MAILZ="true"
    fi
}

install_agent_tools_via_agntz() {
    log_step "Installing agent tools via agntz"
    
    if ! command_exists agntz; then
        log_error "agntz not available. Skipping agent tools installation."
        return 1
    fi
    
    if [[ "$INSTALL_MMRY" == "true" ]]; then
        log_info "Installing mmry..."
        if agntz install mmry 2>/dev/null || cargo install mmry 2>/dev/null; then
            log_success "mmry installed"
        else
            log_warn "Failed to install mmry. You can install it manually later."
        fi
    fi
    
    if [[ "$INSTALL_TRX" == "true" ]]; then
        log_info "Installing trx..."
        if agntz install trx 2>/dev/null || cargo install trx 2>/dev/null; then
            log_success "trx installed"
        else
            log_warn "Failed to install trx. You can install it manually later."
        fi
    fi
    
    if [[ "$INSTALL_MAILZ" == "true" ]]; then
        log_info "Installing mailz..."
        if agntz install mailz 2>/dev/null || cargo install mailz 2>/dev/null; then
            log_success "mailz installed"
        else
            log_warn "Failed to install mailz. You can install it manually later."
        fi
    fi
}

build_octo() {
    log_step "Building Octo components"
    
    cd "$SCRIPT_DIR"
    
    # Build backend
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
    
    # Install binaries
    log_info "Installing binaries to ~/.cargo/bin..."
    (cd backend && cargo install --path .)
    (cd fileserver && cargo install --path .)
    log_success "Binaries installed"
}

# ==============================================================================
# Mode Selection
# ==============================================================================

select_user_mode() {
    log_step "User Mode Selection"
    
    echo
    echo "Octo supports two user modes:"
    echo
    echo "  ${BOLD}Single-user${NC} - Personal deployment"
    echo "    - All sessions use the same workspace"
    echo "    - Simpler setup, no user management"
    echo "    - Best for: personal laptops, single-developer servers"
    echo
    echo "  ${BOLD}Multi-user${NC} - Team deployment"
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
    echo "  ${BOLD}Local${NC} - Native processes"
    echo "    - Runs OpenCode, fileserver, ttyd directly on host"
    echo "    - Lower overhead, faster startup"
    echo "    - Best for: development, single-user, trusted environments"
    echo
    echo "  ${BOLD}Container${NC} - Docker/Podman containers"
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

generate_config() {
    log_step "Generating configuration"
    
    # Create config directories
    mkdir -p "$OCTO_CONFIG_DIR"
    mkdir -p "$OCTO_DATA_DIR"
    mkdir -p "$OPENCODE_CONFIG_DIR"
    
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
    fi
    
    # Generate JWT secret for non-dev mode
    local jwt_secret
    jwt_secret=$(generate_jwt_secret)
    
    # EAVS configuration
    local eavs_enabled="false"
    local eavs_base_url="http://localhost:41800"
    local eavs_container_url="http://host.containers.internal:41800"
    
    if confirm "Enable EAVS LLM proxy integration?" "n"; then
        eavs_enabled="true"
        eavs_base_url=$(prompt_input "EAVS base URL" "$eavs_base_url")
        if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
            eavs_container_url=$(prompt_input "EAVS container URL" "$eavs_container_url")
        fi
    fi
    
    # Linux user isolation (multi-user local mode only)
    local linux_users_enabled="false"
    if [[ "$SELECTED_USER_MODE" == "multi" && "$SELECTED_BACKEND_MODE" == "local" && "$OS" == "linux" ]]; then
        if confirm "Enable Linux user isolation? (requires sudo/root)"; then
            linux_users_enabled="true"
        fi
    fi
    
    # Write config file
    log_info "Writing config to $config_file"
    
    cat > "$config_file" << EOF
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
        cat >> "$config_file" << EOF

[local]
enabled = true
opencode_binary = "opencode"
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

    cat >> "$config_file" << EOF

[eavs]
enabled = $eavs_enabled
base_url = "$eavs_base_url"
EOF

    if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
        echo "container_url = \"$eavs_container_url\"" >> "$config_file"
    fi

    cat >> "$config_file" << EOF

[auth]
dev_mode = $OCTO_DEV_MODE
# jwt_secret = "$jwt_secret"
EOF

    if [[ "$OCTO_DEV_MODE" == "true" && -n "${dev_user_hash:-}" ]]; then
        cat >> "$config_file" << EOF

[[auth.dev_users]]
id = "$dev_user_id"
name = "$dev_user_name"
email = "$dev_user_email"
password_hash = "$dev_user_hash"
role = "admin"
EOF
    fi

    cat >> "$config_file" << EOF

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
    
    # Copy opencode config
    if [[ ! -f "$OPENCODE_CONFIG_DIR/opencode.json" ]]; then
        log_info "Copying OpenCode config template..."
        cp "$TEMPLATES_DIR/opencode/opencode.json" "$OPENCODE_CONFIG_DIR/opencode.json"
        log_success "OpenCode config created at $OPENCODE_CONFIG_DIR/opencode.json"
    fi
    
    # Create workspace directory
    if [[ "$SELECTED_USER_MODE" == "single" ]]; then
        mkdir -p "$WORKSPACE_DIR"
        log_success "Workspace directory created: $WORKSPACE_DIR"
        
        # Copy AGENTS.md template if not exists
        if [[ ! -f "$WORKSPACE_DIR/AGENTS.md" ]]; then
            cp "$TEMPLATES_DIR/opencode/AGENTS.md" "$WORKSPACE_DIR/AGENTS.md"
            log_info "Created default AGENTS.md in workspace"
        fi
    fi
}

# ==============================================================================
# Service Installation
# ==============================================================================

install_service_linux() {
    log_step "Installing systemd service"
    
    if [[ "$SELECTED_USER_MODE" == "single" ]]; then
        # User-level service
        local service_dir="$HOME/.config/systemd/user"
        mkdir -p "$service_dir"
        
        local service_file="$service_dir/octo.service"
        
        cat > "$service_file" << EOF
# Octo Server - User service
# Generated by setup.sh

[Unit]
Description=Octo Server (User Mode)
After=default.target

[Service]
Type=simple
Environment=OCTO_CONFIG=$OCTO_CONFIG_DIR/config.toml
Environment=RUST_LOG=$OCTO_LOG_LEVEL
ExecStart=$HOME/.cargo/bin/octo serve --local-mode
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
        
        # Create octo system user
        if ! id octo &>/dev/null; then
            log_info "Creating octo system user..."
            sudo useradd -r -s /usr/sbin/nologin -d /var/lib/octo octo
        fi
        
        # Create directories
        sudo mkdir -p /var/lib/octo /etc/octo /run/octo
        sudo chown octo:octo /var/lib/octo /run/octo
        
        # Copy config
        sudo cp "$OCTO_CONFIG_DIR/config.toml" /etc/octo/config.toml
        sudo chown octo:octo /etc/octo/config.toml
        
        # Install service file
        local service_file="/etc/systemd/system/octo.service"
        
        sudo tee "$service_file" > /dev/null << EOF
# Octo Server - System service
# Generated by setup.sh

[Unit]
Description=Octo Control Plane Server
After=network.target

[Service]
Type=simple
User=octo
Group=octo
WorkingDirectory=/var/lib/octo
Environment=OCTO_CONFIG=/etc/octo/config.toml
Environment=RUST_LOG=$OCTO_LOG_LEVEL
StateDirectory=octo
RuntimeDirectory=octo
RuntimeDirectoryMode=0755
ConfigurationDirectory=octo
ExecStart=/usr/local/bin/octo serve
ExecStop=/bin/kill -TERM \$MAINPID
TimeoutStopSec=30
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/octo
ReadWritePaths=/run/octo
PrivateTmp=true
AmbientCapabilities=CAP_NET_BIND_SERVICE

[Install]
WantedBy=multi-user.target
EOF
        
        # Copy binary to /usr/local/bin
        sudo cp "$HOME/.cargo/bin/octo" /usr/local/bin/octo
        sudo cp "$HOME/.cargo/bin/fileserver" /usr/local/bin/fileserver
        
        log_success "Service file created: $service_file"
        
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
    
    cat > "$plist_file" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>ai.octo.server</string>

    <key>ProgramArguments</key>
    <array>
        <string>$HOME/.cargo/bin/octo</string>
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
        <string>/usr/local/bin:/usr/bin:/bin:$HOME/.cargo/bin:$HOME/.bun/bin</string>
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
    echo "Configuration:"
    echo "  User mode:    $SELECTED_USER_MODE"
    echo "  Backend mode: $SELECTED_BACKEND_MODE"
    echo "  Config file:  $OCTO_CONFIG_DIR/config.toml"
    echo
    
    if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
        echo "Installed binaries:"
        echo "  octo:       $(which octo 2>/dev/null || echo 'not in PATH')"
        echo "  fileserver: $(which fileserver 2>/dev/null || echo 'not in PATH')"
        echo "  opencode:   $(which opencode 2>/dev/null || echo 'not in PATH')"
        echo "  ttyd:       $(which ttyd 2>/dev/null || echo 'not in PATH')"
        echo
    fi
    
    echo "Shell tools:"
    echo "  tmux:       $(which tmux 2>/dev/null || echo 'not installed')"
    echo "  fd:         $(which fd 2>/dev/null || which fdfind 2>/dev/null || echo 'not installed')"
    echo "  ripgrep:    $(which rg 2>/dev/null || echo 'not installed')"
    echo "  yazi:       $(which yazi 2>/dev/null || echo 'not installed')"
    echo "  zsh:        $(which zsh 2>/dev/null || echo 'not installed')"
    echo "  zoxide:     $(which zoxide 2>/dev/null || echo 'not installed')"
    echo
    
    echo "Agent tools:"
    echo "  agntz:      $(which agntz 2>/dev/null || echo 'not installed')"
    if [[ "$INSTALL_MMRY" == "true" ]]; then
        echo "  mmry:       $(which mmry 2>/dev/null || echo 'not installed')"
    fi
    if [[ "$INSTALL_TRX" == "true" ]]; then
        echo "  trx:        $(which trx 2>/dev/null || echo 'not installed')"
    fi
    if [[ "$INSTALL_MAILZ" == "true" ]]; then
        echo "  mailz:      $(which mailz 2>/dev/null || echo 'not installed')"
    fi
    echo
    
    echo "Next steps:"
    echo
    echo "  1. Start the server:"
    if [[ "$OS" == "linux" && "$SELECTED_USER_MODE" == "single" ]]; then
        echo "     systemctl --user start octo"
        echo "     # or manually:"
    fi
    if [[ "$OS" == "macos" ]]; then
        echo "     launchctl load ~/Library/LaunchAgents/ai.octo.server.plist"
        echo "     # or manually:"
    fi
    echo "     octo serve $([[ "$SELECTED_BACKEND_MODE" == "local" ]] && echo '--local-mode')"
    echo
    echo "  2. Start the frontend dev server:"
    echo "     cd $SCRIPT_DIR/frontend && bun dev"
    echo
    echo "  3. Open the web interface:"
    echo "     http://localhost:3000"
    echo
    
    if [[ "$OCTO_DEV_MODE" == "true" && -n "${dev_user_id:-}" ]]; then
        echo "  4. Login with your dev credentials:"
        echo "     Username: $dev_user_id"
        echo "     Password: (the password you entered)"
        echo
    fi
    
    echo "For more information, see:"
    echo "  - README.md"
    echo "  - deploy/systemd/README.md (Linux systemd setup)"
    echo "  - backend/examples/config.toml (full config reference)"
}

# ==============================================================================
# Main
# ==============================================================================

show_help() {
    cat << EOF
Octo Setup Script

Usage: $0 [OPTIONS]

Options:
  --help              Show this help message
  --non-interactive   Run without prompts (uses defaults/env vars)

Environment Variables:
  OCTO_USER_MODE          single or multi (default: single)
  OCTO_BACKEND_MODE       local or container (default: local)
  OCTO_CONTAINER_RUNTIME  docker, podman, or auto (default: auto)
  OCTO_INSTALL_DEPS       yes or no (default: yes)
  OCTO_INSTALL_SERVICE    yes or no (default: yes)
  OCTO_INSTALL_AGENT_TOOLS yes or no (default: yes)
  OCTO_DEV_MODE           true or false (default: true)
  OCTO_LOG_LEVEL          error, warn, info, debug, trace (default: info)

Shell Tools Installed:
  tmux, fd, ripgrep, yazi, zsh, zoxide

Agent Tools (via agntz):
  agntz   - Agent operations CLI (always installed)
  mmry    - Memory system (optional)
  trx     - Transaction tracking (optional)
  mailz   - Agent messaging (optional)

Examples:
  # Interactive setup (recommended)
  ./setup.sh

  # Non-interactive single-user local setup
  OCTO_USER_MODE=single OCTO_BACKEND_MODE=local ./setup.sh --non-interactive

  # Multi-user container setup on Linux
  OCTO_USER_MODE=multi OCTO_BACKEND_MODE=container ./setup.sh
EOF
}

main() {
    NONINTERACTIVE="false"
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --help|-h)
                show_help
                exit 0
                ;;
            --non-interactive)
                NONINTERACTIVE="true"
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
    fi
    
    # Prerequisites
    check_prerequisites
    
    # Install dependencies
    if [[ "$OCTO_INSTALL_DEPS" == "yes" ]]; then
        # Shell tools (always useful)
        install_shell_tools
        
        if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
            install_opencode
            install_ttyd
        fi
        
        # Agent tools (agntz and optional mmry, trx, mailz)
        if [[ "$OCTO_INSTALL_AGENT_TOOLS" == "yes" ]]; then
            install_agntz
            
            if [[ "$NONINTERACTIVE" != "true" ]]; then
                select_agent_tools
            fi
            
            if [[ "$INSTALL_MMRY" == "true" || "$INSTALL_TRX" == "true" || "$INSTALL_MAILZ" == "true" ]]; then
                install_agent_tools_via_agntz
            fi
        fi
    fi
    
    # Build Octo
    if confirm "Build Octo from source?"; then
        build_octo
    fi
    
    # Generate configuration
    generate_config
    
    # Build container image (if container mode)
    build_container_image
    
    # Install service
    install_service
    
    # Summary
    print_summary
}

main "$@"
