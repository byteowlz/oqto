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
: "${OCTO_INSTALL_AGENT_TOOLS:=yes}"    # yes or no (agntz, mmry, trx)
: "${OCTO_DEV_MODE:=}"                  # true or false (auth dev mode) - empty = prompt
: "${OCTO_LOG_LEVEL:=info}"             # error, warn, info, debug, trace
: "${OCTO_SETUP_CADDY:=}"               # yes or no - empty = prompt
: "${OCTO_DOMAIN:=}"                    # domain for HTTPS (e.g., octo.example.com)

# Agent tools installation tracking
INSTALL_MMRY="false"
INSTALL_TRX="false"
INSTALL_MAILZ="false"

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
# Agent Tools Installation (agntz, mmry, trx)
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
    if confirm "Install mmry (memory system)?"; then
        INSTALL_MMRY="true"
    fi
    
    if confirm "Install trx (task tracking)?"; then
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

# ==============================================================================
# Production Mode Setup
# ==============================================================================

select_deployment_mode() {
    log_step "Deployment Mode Selection"
    
    echo
    echo "Octo can be deployed in two modes:"
    echo
    echo "  ${BOLD}Development${NC} - For local development and testing"
    echo "    - Uses dev_mode authentication (no JWT secret required)"
    echo "    - Preconfigured dev users for easy login"
    echo "    - HTTP only (no TLS)"
    echo "    - Best for: local development, testing"
    echo
    echo "  ${BOLD}Production${NC} - For server deployments"
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
                arch|manjaro|endeavouros)
                    log_info "Installing Caddy via pacman..."
                    sudo pacman -S --noconfirm caddy
                    ;;
                debian|ubuntu|pop|linuxmint)
                    log_info "Installing Caddy via apt..."
                    sudo apt-get install -y debian-keyring debian-archive-keyring apt-transport-https curl
                    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
                    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
                    sudo apt-get update
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
    if [[ "$DOMAIN" == "localhost" ]]; then
        # Local development - no TLS
        sudo tee "$caddyfile" > /dev/null << EOF
# Octo Caddyfile - Local Development
# Generated by setup.sh on $(date)

:80 {
    # Frontend (React app)
    handle /api/* {
        reverse_proxy localhost:${backend_port}
    }
    
    handle /ws/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # WebSocket upgrade for sessions
    @websockets {
        header Connection *Upgrade*
        header Upgrade websocket
    }
    handle @websockets {
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
        sudo tee "$caddyfile" > /dev/null << EOF
# Octo Caddyfile - Production
# Generated by setup.sh on $(date)
# Domain: ${DOMAIN}

${DOMAIN} {
    # Backend API
    handle /api/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # WebSocket connections
    handle /ws/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # WebSocket upgrade handling
    @websockets {
        header Connection *Upgrade*
        header Upgrade websocket
    }
    handle @websockets {
        reverse_proxy localhost:${backend_port}
    }
    
    # Frontend (React app)
    handle {
        reverse_proxy localhost:${frontend_port}
    }
    
    # Security headers
    header {
        X-Content-Type-Options nosniff
        X-Frame-Options DENY
        Referrer-Policy strict-origin-when-cross-origin
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
    
    # Install binaries
    log_info "Installing binaries to ~/.cargo/bin..."
    (cd backend && cargo install --path .)
    (cd fileserver && cargo install --path .)
    
    # Install additional binaries (octo-runner for multi-user, pi-bridge for container Pi, octo-sandbox for sandboxing)
    if [[ -f "$SCRIPT_DIR/backend/target/release/octo-runner" ]]; then
        cp "$SCRIPT_DIR/backend/target/release/octo-runner" "$HOME/.cargo/bin/"
        log_success "octo-runner installed"
    fi
    if [[ -f "$SCRIPT_DIR/backend/target/release/pi-bridge" ]]; then
        cp "$SCRIPT_DIR/backend/target/release/pi-bridge" "$HOME/.cargo/bin/"
        log_success "pi-bridge installed"
    fi
    if [[ -f "$SCRIPT_DIR/backend/target/release/octo-sandbox" ]]; then
        cp "$SCRIPT_DIR/backend/target/release/octo-sandbox" "$HOME/.cargo/bin/"
        log_success "octo-sandbox installed"
    fi
    
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
    local eavs_enabled="false"
    local eavs_base_url="http://localhost:41800"
    local eavs_container_url="http://host.docker.internal:41800"
    
    # LLM Provider configuration
    local llm_provider=""
    local llm_api_key=""
    
    echo
    echo "LLM Provider Configuration:"
    echo
    echo "  Octo needs access to an LLM provider for AI agents."
    echo "  You can either:"
    echo "    1) Use EAVS (LLM proxy) - manages API keys and usage limits"
    echo "    2) Configure API keys directly for a provider"
    echo
    
    if confirm "Use EAVS LLM proxy?" "n"; then
        eavs_enabled="true"
        EAVS_ENABLED="true"
        eavs_base_url=$(prompt_input "EAVS base URL" "$eavs_base_url")
        if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
            eavs_container_url=$(prompt_input "EAVS container URL" "$eavs_container_url")
        fi
    else
        # Direct provider configuration
        echo
        echo "Select LLM provider:"
        echo
        local provider_choice
        provider_choice=$(prompt_choice "Provider:" "anthropic" "openai" "openrouter" "google" "groq")
        llm_provider="$provider_choice"
        
        case "$llm_provider" in
            anthropic)
                echo
                echo "Get your Anthropic API key from: https://console.anthropic.com/"
                if [[ -n "${ANTHROPIC_API_KEY:-}" ]]; then
                    log_info "Found ANTHROPIC_API_KEY in environment"
                    llm_api_key="$ANTHROPIC_API_KEY"
                else
                    llm_api_key=$(prompt_input "Anthropic API key (or press Enter to skip)")
                fi
                ;;
            openai)
                echo
                echo "Get your OpenAI API key from: https://platform.openai.com/api-keys"
                if [[ -n "${OPENAI_API_KEY:-}" ]]; then
                    log_info "Found OPENAI_API_KEY in environment"
                    llm_api_key="$OPENAI_API_KEY"
                else
                    llm_api_key=$(prompt_input "OpenAI API key (or press Enter to skip)")
                fi
                ;;
            openrouter)
                echo
                echo "Get your OpenRouter API key from: https://openrouter.ai/keys"
                if [[ -n "${OPENROUTER_API_KEY:-}" ]]; then
                    log_info "Found OPENROUTER_API_KEY in environment"
                    llm_api_key="$OPENROUTER_API_KEY"
                else
                    llm_api_key=$(prompt_input "OpenRouter API key (or press Enter to skip)")
                fi
                ;;
            google)
                echo
                echo "Get your Google AI API key from: https://aistudio.google.com/app/apikey"
                if [[ -n "${GOOGLE_API_KEY:-}" ]]; then
                    log_info "Found GOOGLE_API_KEY in environment"
                    llm_api_key="$GOOGLE_API_KEY"
                else
                    llm_api_key=$(prompt_input "Google AI API key (or press Enter to skip)")
                fi
                ;;
            groq)
                echo
                echo "Get your Groq API key from: https://console.groq.com/keys"
                if [[ -n "${GROQ_API_KEY:-}" ]]; then
                    log_info "Found GROQ_API_KEY in environment"
                    llm_api_key="$GROQ_API_KEY"
                else
                    llm_api_key=$(prompt_input "Groq API key (or press Enter to skip)")
                fi
                ;;
        esac
        
        # Set global variables for summary
        LLM_PROVIDER="$llm_provider"
        if [[ -n "$llm_api_key" ]]; then
            LLM_API_KEY_SET="true"
        else
            log_warn "No API key configured. You'll need to set the appropriate environment variable before running Octo."
            log_info "Set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, OPENROUTER_API_KEY, GOOGLE_API_KEY, or GROQ_API_KEY"
        fi
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

    # Determine allowed origins for CORS
    local allowed_origins=""
    if [[ "$PRODUCTION_MODE" == "true" && -n "$DOMAIN" && "$DOMAIN" != "localhost" ]]; then
        allowed_origins="allowed_origins = [\"https://${DOMAIN}\"]"
    fi

    cat >> "$config_file" << EOF

[auth]
dev_mode = $OCTO_DEV_MODE
EOF

    # Add JWT secret for production mode (uncommented)
    if [[ "$PRODUCTION_MODE" == "true" ]]; then
        cat >> "$config_file" << EOF
jwt_secret = "$jwt_secret"
EOF
    else
        cat >> "$config_file" << EOF
# jwt_secret = "$jwt_secret"
EOF
    fi
    
    # Add CORS origins if configured
    if [[ -n "$allowed_origins" ]]; then
        echo "$allowed_origins" >> "$config_file"
    fi

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

    # Pi (Main Chat) configuration
    # Determine Pi runtime mode based on backend mode and user mode
    local pi_runtime_mode="local"
    if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
        pi_runtime_mode="container"
    elif [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
        pi_runtime_mode="runner"
    fi

    # Set default provider based on selection
    local default_provider="anthropic"
    local default_model="claude-sonnet-4-20250514"
    if [[ -n "$llm_provider" ]]; then
        default_provider="$llm_provider"
        case "$llm_provider" in
            anthropic)
                default_model="claude-sonnet-4-20250514"
                ;;
            openai)
                default_model="gpt-4o"
                ;;
            openrouter)
                default_model="anthropic/claude-3.5-sonnet"
                ;;
            google)
                default_model="gemini-1.5-pro"
                ;;
            groq)
                default_model="llama-3.3-70b-versatile"
                ;;
        esac
    fi

    cat >> "$config_file" << EOF

[pi]
enabled = true
executable = "pi"
default_provider = "$default_provider"
default_model = "$default_model"
runtime_mode = "$pi_runtime_mode"
EOF

    # Add runner socket pattern for multi-user Linux mode
    if [[ "$pi_runtime_mode" == "runner" ]]; then
        cat >> "$config_file" << 'EOF'

[local]
runner_socket_pattern = "/run/octo/runner-sockets/{user}/octo-runner.sock"
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
    
    # Create environment file for API keys (if not using EAVS)
    if [[ "$eavs_enabled" == "false" && -n "$llm_api_key" ]]; then
        local env_file="$OCTO_CONFIG_DIR/env"
        log_info "Writing API key to $env_file"
        
        case "$llm_provider" in
            anthropic)
                echo "ANTHROPIC_API_KEY=$llm_api_key" > "$env_file"
                ;;
            openai)
                echo "OPENAI_API_KEY=$llm_api_key" > "$env_file"
                ;;
            openrouter)
                echo "OPENROUTER_API_KEY=$llm_api_key" > "$env_file"
                ;;
            google)
                echo "GOOGLE_API_KEY=$llm_api_key" > "$env_file"
                ;;
            groq)
                echo "GROQ_API_KEY=$llm_api_key" > "$env_file"
                ;;
        esac
        
        chmod 600 "$env_file"
        log_success "API key saved to $env_file"
        log_info "Source this file before running Octo: source $env_file"
    fi
    
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
    
    # Save admin credentials for post-setup user creation
    if [[ "$PRODUCTION_MODE" == "true" && -n "$ADMIN_USERNAME" ]]; then
        local creds_file="$OCTO_CONFIG_DIR/.admin_setup"
        cat > "$creds_file" << EOF
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
    server_user=$(whoami)
    
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
    local sudoers_content="# Octo Multi-User Process Isolation
# Generated by setup.sh on $(date)
# Allows the octo server user to manage isolated user accounts

# User management helpers
Cmnd_Alias OCTO_USERADD = /usr/sbin/useradd * ${user_prefix}*
Cmnd_Alias OCTO_USERMOD = /usr/sbin/usermod * ${user_prefix}*
Cmnd_Alias OCTO_USERDEL = /usr/sbin/userdel * ${user_prefix}*
Cmnd_Alias OCTO_GROUPADD = /usr/sbin/groupadd ${octo_group}
Cmnd_Alias OCTO_CHOWN = /usr/bin/chown -R ${user_prefix}*:${octo_group} *

# User management - create/modify/delete octo_* users
${server_user} ALL=(root) NOPASSWD: OCTO_USERADD, OCTO_USERMOD, OCTO_USERDEL, OCTO_GROUPADD

# Process execution - run commands as octo_* users
${server_user} ALL=(${user_prefix}*) NOPASSWD: ALL

# Workspace ownership updates (required for linux_users chown)
${server_user} ALL=(root) NOPASSWD: OCTO_CHOWN

# Alternative: use 'su' to switch users (some systems prefer this)
${server_user} ALL=(root) NOPASSWD: /usr/bin/su - ${user_prefix}* -c *
"
    
    # Write sudoers file (use visudo -c to validate)
    echo "$sudoers_content" | sudo tee "$sudoers_file" > /dev/null
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
    
    sudo tee "$sandbox_config" > /dev/null << 'EOF'
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
    "~/.local/share/mailz",
    # Agent tools - config directories
    "~/.config/skdlr",
    "~/.config/mmry",
    "~/.config/mailz",
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
EnvironmentFile=-$OCTO_CONFIG_DIR/env
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
        
        # Copy binaries to /usr/local/bin
        sudo cp "$HOME/.cargo/bin/octo" /usr/local/bin/octo
        sudo cp "$HOME/.cargo/bin/fileserver" /usr/local/bin/fileserver
        if [[ -f "$HOME/.cargo/bin/octo-runner" ]]; then
            sudo cp "$HOME/.cargo/bin/octo-runner" /usr/local/bin/octo-runner
        fi
        
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
    sudo tee "$runner_service" > /dev/null << 'EOF'
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
    sudo tee "$tmpfiles_conf" > /dev/null << 'EOF'
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
    echo "  User mode:       $SELECTED_USER_MODE"
    echo "  Backend mode:    $SELECTED_BACKEND_MODE"
    echo "  Deployment mode: $([[ "$PRODUCTION_MODE" == "true" ]] && echo "Production" || echo "Development")"
    echo "  Config file:     $OCTO_CONFIG_DIR/config.toml"
    echo
    
    if [[ "$PRODUCTION_MODE" == "true" ]]; then
        echo "Security:"
        echo "  JWT secret:    configured (64 characters)"
        echo "  Admin user:    $ADMIN_USERNAME"
        echo "  Admin email:   $ADMIN_EMAIL"
        if [[ "$NONINTERACTIVE" == "true" ]]; then
            echo -e "  ${YELLOW}Admin password: $ADMIN_PASSWORD${NC}"
            echo -e "  ${RED}SAVE THIS PASSWORD - it will not be shown again!${NC}"
        fi
        echo
        
        if [[ "$SETUP_CADDY" == "yes" ]]; then
            echo "Reverse Proxy:"
            echo "  Caddy:       installed"
            echo "  Domain:      $DOMAIN"
            if [[ "$DOMAIN" != "localhost" ]]; then
                echo "  HTTPS:       enabled (automatic via Let's Encrypt)"
            fi
            echo "  Config:      /etc/caddy/Caddyfile"
            echo
        fi
    fi
    
    echo "LLM Configuration:"
    if [[ "$EAVS_ENABLED" == "true" ]]; then
        echo "  Mode:         EAVS proxy"
    elif [[ -n "$LLM_PROVIDER" ]]; then
        echo "  Provider:     $LLM_PROVIDER"
        if [[ "$LLM_API_KEY_SET" == "true" ]]; then
            echo "  API key:      configured (saved to $OCTO_CONFIG_DIR/env)"
        else
            echo "  API key:      NOT SET - you need to configure this!"
        fi
    else
        echo "  Provider:     not configured"
    fi
    echo
    
    echo "Installed binaries:"
    echo "  octo:         $(which octo 2>/dev/null || echo 'not in PATH')"
    echo "  fileserver:   $(which fileserver 2>/dev/null || echo 'not in PATH')"
    if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
        echo "  opencode:     $(which opencode 2>/dev/null || echo 'not in PATH')"
        echo "  ttyd:         $(which ttyd 2>/dev/null || echo 'not in PATH')"
    fi
    if [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
        echo "  octo-runner:  $(which octo-runner 2>/dev/null || echo 'not in PATH')"
    fi
    if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
        echo "  pi-bridge:    $(which pi-bridge 2>/dev/null || echo 'not in PATH')"
    fi
    echo
    
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
    
    if [[ "$PRODUCTION_MODE" == "true" ]]; then
        # Production mode instructions
        local step=1
        
        if [[ "$SETUP_CADDY" == "yes" ]]; then
            echo "  $step. Start Caddy reverse proxy:"
            if [[ "$OS" == "linux" ]]; then
                echo "     sudo systemctl start caddy"
            else
                echo "     sudo caddy start --config /etc/caddy/Caddyfile"
            fi
            echo
            ((step++))
        fi
        
        echo "  $step. Start the Octo server:"
        if [[ "$OS" == "linux" ]]; then
            if [[ "$SELECTED_USER_MODE" == "single" ]]; then
                echo "     systemctl --user start octo"
            else
                echo "     sudo systemctl start octo"
            fi
        else
            echo "     launchctl load ~/Library/LaunchAgents/ai.octo.server.plist"
        fi
        echo "     # or manually:"
        echo "     octo serve $([[ "$SELECTED_BACKEND_MODE" == "local" ]] && echo '--local-mode')"
        echo
        ((step++))
        
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
        echo "     octo invites create --uses 1"
        echo "     # Or use the admin interface"
        echo
    else
        # Development mode instructions
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
  OCTO_DEV_MODE           true or false (default: prompt user)
  OCTO_LOG_LEVEL          error, warn, info, debug, trace (default: info)
  OCTO_SETUP_CADDY        yes or no (default: prompt user in production mode)
  OCTO_DOMAIN             domain for HTTPS (e.g., octo.example.com)

LLM Provider API Keys (set one of these, or use EAVS):
  ANTHROPIC_API_KEY       Anthropic Claude API key
  OPENAI_API_KEY          OpenAI API key
  OPENROUTER_API_KEY      OpenRouter API key
  GOOGLE_API_KEY          Google AI API key
  GROQ_API_KEY            Groq API key

Shell Tools Installed:
  tmux, fd, ripgrep, yazi, zsh, zoxide

Agent Tools:
  agntz   - Agent operations CLI (memory, issues, mail, reservations)
  mmry    - Memory system (optional, integrated with Octo)
  trx     - Task tracking (optional, integrated with Octo)
  mailz   - Agent messaging (optional)

Other Tools:
  opencode - OpenCode AI agent CLI (local mode)
  ttyd    - Web terminal
  pi      - Main chat interface

For detailed documentation on all prerequisites and components, see SETUP.md

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
            install_opencode
            install_ttyd
        fi
        
        # Agent tools (agntz and optional mmry, trx)
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
    
    # Install service
    install_service
    
    # Create admin user in database (production mode)
    create_admin_user_db
    
    # Summary
    print_summary
}

main "$@"
