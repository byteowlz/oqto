#!/usr/bin/env bash
#
# Oqto Setup Script
# Comprehensive setup and onboarding for the Oqto AI Agent Workspace Platform
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

set -uo pipefail

# ==============================================================================
# Configuration and Defaults
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEMPLATES_DIR="${SCRIPT_DIR}/templates"
# Use SSH for private repos (allows SSH key authentication)
ONBOARDING_TEMPLATES_REPO_DEFAULT="git@github.com:byteowlz/oqto-templates.git"
EXTERNAL_REPOS_DIR_DEFAULT="/usr/local/share/oqto/external-repos"
ONBOARDING_TEMPLATES_PATH_DEFAULT="/usr/share/oqto/oqto-templates/dotfiles_users/"
PROJECT_TEMPLATES_PATH_DEFAULT="/usr/share/oqto/oqto-templates/agents/"

# Default values (can be overridden by environment variables)
: "${OQTO_USER_MODE:=single}"        # single or multi
: "${OQTO_BACKEND_MODE:=local}"      # local or container
: "${OQTO_CONTAINER_RUNTIME:=auto}"  # docker, podman, or auto
: "${OQTO_INSTALL_DEPS:=yes}"        # yes or no
: "${OQTO_INSTALL_SERVICE:=yes}"     # yes or no
: "${OQTO_INSTALL_AGENT_TOOLS:=yes}" # yes or no (agntz, mmry, trx)
: "${OQTO_DEV_MODE:=}"               # true or false (auth dev mode) - empty = prompt
: "${OQTO_LOG_LEVEL:=info}"          # error, warn, info, debug, trace
: "${OQTO_SETUP_CADDY:=}"            # yes or no - empty = prompt
: "${OQTO_DOMAIN:=}"                 # domain for HTTPS (e.g., oqto.example.com)

# Server hardening options (Linux only, requires root)
: "${OQTO_HARDEN_SERVER:=}"         # yes or no - empty = prompt in production mode
: "${OQTO_SSH_PORT:=22}"            # SSH port (change if needed)
: "${OQTO_SETUP_FIREWALL:=yes}"     # Configure UFW/firewalld
: "${OQTO_SETUP_FAIL2BAN:=yes}"     # Install and configure fail2ban
: "${OQTO_HARDEN_SSH:=yes}"         # Apply SSH hardening config
: "${OQTO_SETUP_AUTO_UPDATES:=yes}" # Enable automatic security updates
: "${OQTO_HARDEN_KERNEL:=yes}"      # Apply kernel security parameters

# Additional env vars for web configurator (oqto.dev/setup)
: "${OQTO_PROVIDERS:=}"              # comma-separated: anthropic,openai,google
: "${OQTO_TOOLS:=}"                  # comma-separated tool list (when not --all-tools)
: "${OQTO_INSTALL_ALL_TOOLS:=}"      # yes or no - install all agent tools
: "${OQTO_WORKSPACE_DIR:=}"          # workspace directory override
: "${OQTO_ADMIN_USER:=}"             # admin username (default: admin)
: "${OQTO_ADMIN_EMAIL:=}"            # admin email

# Agent tools installation tracking
INSTALL_MMRY="false"
INSTALL_ALL_TOOLS="false"

# Custom provider definitions from oqto.setup.toml
CUSTOM_PROVIDERS=()
# shellcheck disable=SC2034
declare -A CP_TYPE CP_BASE_URL CP_API_KEY CP_DEPLOYMENT CP_API_VERSION CP_AWS_REGION CP_GCP_PROJECT CP_GCP_LOCATION CP_TEST_MODEL

# LLM provider configuration (set during generate_config)
LLM_PROVIDER=""
LLM_API_KEY_SET="false"
EAVS_ENABLED="false"
CONFIGURED_PROVIDERS=""

# Production configuration (set during setup)
PRODUCTION_MODE="false"
SETUP_CADDY="false"
DOMAIN=""
JWT_SECRET=""
ADMIN_USERNAME=""
# ADMIN_PASSWORD is never persisted -- prompted inline when needed
ADMIN_EMAIL=""

# Dev user configuration (set during generate_config)
dev_user_id=""
dev_user_name=""
dev_user_email=""
# dev_user_password is never persisted -- prompted inline when needed

# Paths (XDG compliant)
: "${XDG_CONFIG_HOME:=$HOME/.config}"
: "${XDG_DATA_HOME:=$HOME/.local/share}"
: "${XDG_STATE_HOME:=$HOME/.local/state}"

OQTO_CONFIG_DIR="${XDG_CONFIG_HOME}/oqto"
OQTO_DATA_DIR="${XDG_DATA_HOME}/oqto"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

# ==============================================================================
# Setup State Persistence
# ==============================================================================
#
# Saves all interactive decisions to a state file so re-runs don't require
# re-entering everything. State is stored in ~/.config/oqto/setup-state.env
# and loaded automatically on subsequent runs.
#
# Usage:
#   ./setup.sh              # Loads previous state, prompts to reuse
#   ./setup.sh --fresh      # Ignore saved state, start from scratch
#

SETUP_STATE_FILE="${XDG_CONFIG_HOME}/oqto/setup-state.env"

# Keys that are persisted (order matters for display)
SETUP_STATE_KEYS=(
  SELECTED_USER_MODE
  SELECTED_BACKEND_MODE
  PRODUCTION_MODE
  OQTO_DEV_MODE
  WORKSPACE_DIR
  DOMAIN
  SETUP_CADDY
  ADMIN_USERNAME
  ADMIN_EMAIL
  dev_user_id
  dev_user_name
  dev_user_email
  INSTALL_ALL_TOOLS
  INSTALL_MMRY
  OQTO_HARDEN_SERVER
  OQTO_SSH_PORT
  OQTO_SETUP_FIREWALL
  OQTO_SETUP_FAIL2BAN
  OQTO_HARDEN_SSH
  OQTO_SETUP_AUTO_UPDATES
  OQTO_HARDEN_KERNEL
  CONTAINER_RUNTIME
  JWT_SECRET
  EAVS_MASTER_KEY
  CONFIGURED_PROVIDERS
)

# Save current decisions to state file
save_setup_state() {
  mkdir -p "$(dirname "$SETUP_STATE_FILE")"

  {
    echo "# Oqto setup state - generated $(date)"
    echo "# This file is loaded on re-runs to avoid re-entering decisions."
    echo "# Delete this file or run ./setup.sh --fresh to start over."
    echo ""
    for key in "${SETUP_STATE_KEYS[@]}"; do
      local val="${!key:-}"
      if [[ -n "$val" ]]; then
        echo "${key}=$(printf '%q' "$val")"
      fi
    done
  } >"$SETUP_STATE_FILE"
  chmod 600 "$SETUP_STATE_FILE"
  log_success "Setup state saved to $SETUP_STATE_FILE"
}

# Load previous state and offer to reuse it
load_setup_state() {
  if [[ ! -f "$SETUP_STATE_FILE" ]]; then
    return 1
  fi

  echo -e "${BOLD}Previous setup state found:${NC}"
  echo ""

  # Show key decisions (skip secrets)
  local secrets_regex="^(JWT_SECRET|EAVS_MASTER_KEY)$"
  while IFS='=' read -r key val; do
    # Skip comments and empty lines
    [[ "$key" =~ ^#.*$ || -z "$key" ]] && continue
    # Unescape the value
    val=$(eval "echo $val" 2>/dev/null || echo "$val")
    if [[ "$key" =~ $secrets_regex ]]; then
      echo -e "  ${CYAN}${key}${NC} = ****"
    else
      echo -e "  ${CYAN}${key}${NC} = ${val}"
    fi
  done <"$SETUP_STATE_FILE"

  # Show completed steps
  if [[ -f "$SETUP_STEPS_FILE" ]]; then
    local step_count
    step_count=$(wc -l <"$SETUP_STEPS_FILE")
    echo -e "  ${GREEN}Completed steps:${NC} $step_count"
    echo ""
  fi

  return 0
}

# Source the state file to restore variables
apply_setup_state() {
  if [[ -f "$SETUP_STATE_FILE" ]]; then
    # Fix known typos from previous versions before sourcing
    # Fix known typos and outdated defaults from previous versions
    sed -i 's|/home/{user_id/oqto}|/home/{linux_username}/oqto|g' "$SETUP_STATE_FILE" 2>/dev/null || true
    sed -i 's|/home/{user_id}/oqto|/home/{linux_username}/oqto|g' "$SETUP_STATE_FILE" 2>/dev/null || true
    # shellcheck source=/dev/null
    source "$SETUP_STATE_FILE"
    log_success "Loaded previous setup state"
  fi
}

# ==============================================================================
# Step Tracking
# ==============================================================================
#
# Tracks which setup steps have completed so re-runs skip finished work.
# Steps are stored in ~/.config/oqto/setup-steps-done alongside the state file.
# Use --fresh to clear completed steps and start over.
#

SETUP_STEPS_FILE="${XDG_CONFIG_HOME}/oqto/setup-steps-done"

# Check if a step has already been completed
step_done() {
  local step="$1"
  [[ -f "$SETUP_STEPS_FILE" ]] && grep -qxF "$step" "$SETUP_STEPS_FILE"
}

# Mark a step as completed
mark_step_done() {
  local step="$1"
  mkdir -p "$(dirname "$SETUP_STEPS_FILE")"
  if ! step_done "$step"; then
    echo "$step" >>"$SETUP_STEPS_FILE"
  fi
}

# Run a step if not already completed; mark done on success
# Usage: run_step "step_name" "description" command [args...]
run_step() {
  local step="$1"
  local desc="$2"
  shift 2

  if step_done "$step"; then
    log_success "Already done: $desc"
    return 0
  fi

  "$@"
  local rc=$?
  if [[ $rc -eq 0 ]]; then
    mark_step_done "$step"
  else
    log_warn "Step failed (non-fatal): $desc"
  fi
}

# Run a step unconditionally (always executes, never skipped).
# Used for steps like building where stale artifacts cause subtle bugs.
# Returns non-zero on failure so callers can decide whether to abort.
run_step_always() {
  local step="$1"
  local desc="$2"
  shift 2

  log_info "Running: $desc"
  "$@"
  local rc=$?
  if [[ $rc -eq 0 ]]; then
    mark_step_done "$step"
  else
    log_error "Step failed: $desc"
    return 1
  fi
}

# Clear all completed steps (used with --fresh)
clear_steps() {
  rm -f "$SETUP_STEPS_FILE"
}

# Load oqto.setup.toml config file and set environment variables.
# This is a simple TOML parser that handles the flat structure generated
# by the web configurator at oqto.dev/setup.
load_setup_config() {
  local config_file="$1"
  local current_section=""

  local current_provider=""

  while IFS= read -r line; do
    # Strip comments and whitespace
    line="${line%%#*}"
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"
    [[ -z "$line" ]] && continue

    # Section header
    if [[ "$line" =~ ^\[([a-z_]+)\]$ ]]; then
      current_section="${BASH_REMATCH[1]}"
      current_provider=""
      continue
    fi

    # Custom provider section: [providers.<name>]
    if [[ "$line" =~ ^\[providers\.([a-zA-Z0-9_-]+)\]$ ]]; then
      current_section="providers.custom"
      current_provider="${BASH_REMATCH[1]}"
      if [[ ! " ${CUSTOM_PROVIDERS[*]} " =~ " ${current_provider} " ]]; then
        CUSTOM_PROVIDERS+=("${current_provider}")
      fi
      continue
    fi

    # Key = value
    if [[ "$line" =~ ^([a-z_]+)[[:space:]]*=[[:space:]]*(.+)$ ]]; then
      local key="${BASH_REMATCH[1]}"
      local val="${BASH_REMATCH[2]}"

      # Strip quotes from string values
      val="${val#\"}"
      val="${val%\"}"

      # Custom provider fields
      if [[ "$current_section" == "providers.custom" && -n "$current_provider" ]]; then
        case "$key" in
          type) CP_TYPE["$current_provider"]="$val" ;;
          base_url) CP_BASE_URL["$current_provider"]="$val" ;;
          api_key) CP_API_KEY["$current_provider"]="$val" ;;
          deployment) CP_DEPLOYMENT["$current_provider"]="$val" ;;
          api_version) CP_API_VERSION["$current_provider"]="$val" ;;
          aws_region) CP_AWS_REGION["$current_provider"]="$val" ;;
          gcp_project) CP_GCP_PROJECT["$current_provider"]="$val" ;;
          gcp_location) CP_GCP_LOCATION["$current_provider"]="$val" ;;
          test_model) CP_TEST_MODEL["$current_provider"]="$val" ;;
        esac
        continue
      fi

      case "${current_section}.${key}" in
        deployment.user_mode)       OQTO_USER_MODE="$val"; SELECTED_USER_MODE="$val" ;;
        deployment.backend_mode)    OQTO_BACKEND_MODE="$val"; SELECTED_BACKEND_MODE="$val" ;;
        deployment.container_runtime) OQTO_CONTAINER_RUNTIME="$val" ;;
        deployment.workspace_dir)   WORKSPACE_DIR="$val" ;;
        network.log_level)          OQTO_LOG_LEVEL="$val" ;;
        network.caddy)              [[ "$val" == "true" ]] && SETUP_CADDY="yes" && OQTO_SETUP_CADDY="yes" ;;
        network.domain)             DOMAIN="$val"; OQTO_DOMAIN="$val" ;;
        admin.username)             ADMIN_USERNAME="$val" ;;
        admin.email)                ADMIN_EMAIL="$val" ;;
        providers.enabled)
          # Parse TOML array: ["anthropic", "openai"]
          val="${val#[}"
          val="${val%]}"
          CONFIGURED_PROVIDERS=""
          local IFS=','
          for provider in $val; do
            provider="${provider#"${provider%%[![:space:]]*}"}"
            provider="${provider%"${provider##*[![:space:]]}"}"
            provider="${provider#\"}"
            provider="${provider%\"}"
            [[ -n "$provider" ]] && CONFIGURED_PROVIDERS="${CONFIGURED_PROVIDERS} ${provider}"
          done
          CONFIGURED_PROVIDERS="${CONFIGURED_PROVIDERS# }"
          ;;
        tools.install_all)
          if [[ "$val" == "true" ]]; then
            INSTALL_ALL_TOOLS="true"
            INSTALL_MMRY="true"
            OQTO_INSTALL_AGENT_TOOLS="yes"
          fi
          ;;
        tools.searxng)              [[ "$val" == "true" ]] && INSTALL_SEARXNG="true" ;;
        hardening.enabled)
          if [[ "$val" == "true" ]]; then
            OQTO_HARDEN_SERVER="yes"
          else
            OQTO_HARDEN_SERVER="no"
          fi
          ;;
        hardening.ssh_port)         OQTO_SSH_PORT="$val" ;;
        hardening.firewall)         [[ "$val" == "true" ]] && OQTO_SETUP_FIREWALL="yes" || OQTO_SETUP_FIREWALL="no" ;;
        hardening.fail2ban)         [[ "$val" == "true" ]] && OQTO_SETUP_FAIL2BAN="yes" || OQTO_SETUP_FAIL2BAN="no" ;;
        hardening.ssh_hardening)    [[ "$val" == "true" ]] && OQTO_HARDEN_SSH="yes" || OQTO_HARDEN_SSH="no" ;;
        hardening.auto_updates)     [[ "$val" == "true" ]] && OQTO_SETUP_AUTO_UPDATES="yes" || OQTO_SETUP_AUTO_UPDATES="no" ;;
        hardening.kernel_security)  [[ "$val" == "true" ]] && OQTO_HARDEN_KERNEL="yes" || OQTO_HARDEN_KERNEL="no" ;;
      esac
    fi
  done < "$config_file"

  log_success "Config loaded: mode=${SELECTED_USER_MODE:-single}, providers=${CONFIGURED_PROVIDERS:-none}"
}

# Run a step with verification: skip only if both marked done AND verify passes
# Usage: verify_or_rerun "step_name" "description" "verify_cmd" install_func
verify_or_rerun() {
  local step="$1"
  local desc="$2"
  local verify="$3"
  local func="$4"

  if step_done "$step" && eval "$verify" &>/dev/null; then
    log_success "Already done: $desc"
    return 0
  fi

  # Clear stale marker if verify failed
  if step_done "$step"; then
    log_warn "$desc marked done but verification failed, re-running..."
    sed -i "/^${step}$/d" "$SETUP_STEPS_FILE" 2>/dev/null || true
  fi

  run_step "$step" "$desc" "$func"
}

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

  # Bun (for frontend and pi)
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

  # Create a global wrapper so all platform users can run pi.
  # bun global installs go to ~/.bun/install/global/ which is per-user,
  # so we need a wrapper script that uses the global bun binary.
  local pi_module
  pi_module="$(readlink -f "$HOME/.bun/bin/pi" 2>/dev/null || true)"
  if [[ -n "$pi_module" && -f "$pi_module" ]]; then
    sudo tee /usr/local/bin/pi >/dev/null <<PIEOF
#!/usr/bin/env bash
exec /usr/local/bin/bun run "$pi_module" "\$@"
PIEOF
    sudo chmod 755 /usr/local/bin/pi
    log_success "pi available at /usr/local/bin: $(/usr/local/bin/pi --version 2>/dev/null || echo 'installed')"
  else
    log_warn "Could not find pi module path. Pi may not be globally accessible."
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

    # Remove files that should not be in the install target
    rm -f "$dest_dir/package.json" "$dest_dir/install.sh"

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

  # Copy to target with sudo (system location)
  log_info "Installing templates to $target_path..."
  sudo mkdir -p "$(dirname "$target_path")"
  sudo rm -rf "$target_path"
  sudo cp -r "$temp_clone_dir" "$target_path"
  sudo chmod -R a+rX "$target_path" >/dev/null 2>&1 || true

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

# ==============================================================================
# ==============================================================================
# Agent Tools Installation
# ==============================================================================
#
# Tools for AI agents in the Oqto platform:
#
#   agntz   - Agent toolkit (wraps other tools, file reservations, etc.)
#   mmry    - Memory storage and semantic search
#   trx     - Issue/task tracking
#   scrpr   - Web content extraction (readability, Tavily, Jina)
#   tmpltr  - Document generation from templates (Typst)
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

# Read a tool version from dependencies.toml
# Usage: get_dep_version hstry -> "0.4.4"
get_dep_version() {
  local tool="$1"
  local deps_file="${SCRIPT_DIR}/dependencies.toml"
  if [[ ! -f "$deps_file" ]]; then
    echo ""
    return
  fi
  # Simple TOML parser: find "tool = "version"" under [byteowlz] section
  sed -n '/^\[byteowlz\]/,/^\[/{s/^'"$tool"' *= *"\(.*\)"/\1/p}' "$deps_file" | head -1
}

# Detect platform for GitHub release downloads
get_release_target() {
  local arch
  arch=$(uname -m)
  local os
  os=$(uname -s)

  case "$os" in
  Linux)
    case "$arch" in
    x86_64) echo "x86_64-unknown-linux-gnu" ;;
    aarch64) echo "aarch64-unknown-linux-gnu" ;;
    *) echo "" ;;
    esac
    ;;
  Darwin)
    case "$arch" in
    x86_64) echo "x86_64-apple-darwin" ;;
    arm64) echo "aarch64-apple-darwin" ;;
    *) echo "" ;;
    esac
    ;;
  *) echo "" ;;
  esac
}

# Download pre-built binary from GitHub releases.
# Falls back to cargo install if download fails or no release exists.
# Usage: download_or_build_tool <binary> <repo> [package]
#   binary:  name of the binary to install (e.g., "hstry")
#   repo:    GitHub repo name (e.g., "hstry")
#   package: package name for multi-binary repos (e.g., "hstry-cli")
download_or_build_tool() {
  local tool="$1"
  local repo="${2:-$tool}"
  local pkg="${3:-}"      # package name for multi-binary Rust repos
  local lang="${4:-rust}" # "rust" or "go"

  local version
  version=$(get_dep_version "$repo")
  local target
  target=$(get_release_target)

  # Try downloading pre-built binary from GitHub releases
  if [[ -n "$version" && "$version" != "latest" && -n "$target" ]]; then
    local tag="v${version}"
    local tmpdir
    tmpdir=$(mktemp -d)

    # Rust repos: repo-vtag-target.tar.gz  (e.g. hstry-v0.4.4-x86_64-unknown-linux-gnu.tar.gz)
    # Go repos:   repo_OS_arch.tar.gz      (e.g. sx_Linux_x86_64.tar.gz)
    local -a urls=()

    # Rust-style URL
    urls+=("${BYTEOWLZ_GITHUB}/${repo}/releases/download/${tag}/${repo}-${tag}-${target}.tar.gz")

    # Go-style URL (goreleaser convention)
    local go_os go_arch
    case "$target" in
    x86_64-unknown-linux-gnu)
      go_os="Linux"
      go_arch="x86_64"
      ;;
    aarch64-unknown-linux-gnu)
      go_os="Linux"
      go_arch="arm64"
      ;;
    x86_64-apple-darwin)
      go_os="Darwin"
      go_arch="x86_64"
      ;;
    aarch64-apple-darwin)
      go_os="Darwin"
      go_arch="arm64"
      ;;
    esac
    if [[ -n "$go_os" ]]; then
      urls+=("${BYTEOWLZ_GITHUB}/${repo}/releases/download/${tag}/${repo}_${go_os}_${go_arch}.tar.gz")
    fi

    log_info "Downloading $tool $tag for $target..."

    for url in "${urls[@]}"; do
      if curl -fsSL "$url" | tar xz -C "$tmpdir" 2>/dev/null; then
        if [[ -x "$tmpdir/$tool" ]]; then
          install_binary_global "$tmpdir/$tool" "$tool"
          rm -rf "$tmpdir"
          log_success "$tool $tag installed from release"
          return 0
        fi
      fi
    done

    rm -rf "$tmpdir"
    log_info "No pre-built binary available, building from source..."
  fi

  # Fall back to building from source
  if [[ "$lang" == "go" ]]; then
    install_go_tool "$tool" "$repo"
  else
    install_rust_tool "$tool" "$repo" "$pkg"
  fi
}

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
  local pkg="${3:-}"       # package name for multi-binary repos (optional)

  if ! command_exists cargo; then
    log_error "Cargo not available. Cannot install $tool."
    return 1
  fi

  # Always install from git to get the latest compatible version.
  # byteowlz tools are tightly coupled and not published to crates.io.
  log_info "Installing $tool from git (latest)..."

  local tmpdir
  tmpdir=$(mktemp -d)
  trap "rm -rf '$tmpdir'" RETURN

  # Special-case ignr to avoid native-tls by forcing rustls-tls
  if [[ "$tool" == "ignr" && -z "$pkg" ]]; then
    log_info "Installing ignr with rustls-tls (avoids OpenSSL)"
    local ignr_dir="$tmpdir/src"
    if git clone --depth 1 "${BYTEOWLZ_GITHUB}/${repo}.git" "$ignr_dir" 2>/dev/null; then
      perl -pi -e 's/reqwest = \{ version = "0\.12", features = \["blocking"\] \}/reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls"] }/' "$ignr_dir/Cargo.toml"
      if cargo install --path "$ignr_dir" --root "$tmpdir" 2>&1 | tail -5; then
        if [[ -x "$tmpdir/bin/$tool" ]]; then
          install_binary_global "$tmpdir/bin/$tool" "$tool"
          return 0
        fi
      fi
    fi
  fi

  # Build cargo install args
  local -a cargo_args=(--git "${BYTEOWLZ_GITHUB}/${repo}.git" --root "$tmpdir")
  if [[ -n "$pkg" ]]; then
    # Multi-binary repo: specify both the package and the binary name
    cargo_args+=(-p "$pkg" --bin "$tool")
  fi

  # Install from GitHub (always latest main branch)
  if cargo install "${cargo_args[@]}" 2>&1 | tail -5; then
    if [[ -x "$tmpdir/bin/$tool" ]]; then
      install_binary_global "$tmpdir/bin/$tool" "$tool"
      return 0
    fi
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
    local -a local_args=(--root "$tmpdir")
    if [[ -n "$pkg" && -d "$local_path/crates/$pkg" ]]; then
      # Multi-binary workspace: point --path to the specific package crate
      local_args+=(--path "$local_path/crates/$pkg")
    else
      local_args+=(--path "$local_path")
    fi
    if cargo install "${local_args[@]}" 2>&1 | tail -5; then
      if [[ -x "$tmpdir/bin/$tool" ]]; then
        install_binary_global "$tmpdir/bin/$tool" "$tool"
        return 0
      fi
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
  download_or_build_tool agntz
}

install_typst_from_cargo() {
  if ! command_exists cargo; then
    log_warn "Cargo not available; cannot install typst"
    return 1
  fi

  local tmpdir
  tmpdir=$(mktemp -d)
  trap "rm -rf '$tmpdir'" RETURN

  if cargo install typst-cli --locked --root "$tmpdir" 2>&1 | tail -3; then
    if [[ -x "$tmpdir/bin/typst" ]]; then
      sudo install -m 755 "$tmpdir/bin/typst" "${TOOLS_INSTALL_DIR}/typst"
      log_success "typst installed"
      return 0
    fi
  fi

  log_warn "typst cargo install failed"
  return 1
}

install_typst() {
  # typst is a dependency of tmpltr (document generation)
  if command_exists typst; then
    log_success "typst already installed: $(typst --version 2>/dev/null | head -1)"
    return 0
  fi

  log_info "Installing typst..."
  local arch
  arch=$(uname -m)
  local os
  os=$(uname -s | tr '[:upper:]' '[:lower:]')

  local target=""
  case "${os}-${arch}" in
    linux-x86_64)  target="x86_64-unknown-linux-musl" ;;
    linux-aarch64) target="aarch64-unknown-linux-musl" ;;
    darwin-x86_64) target="x86_64-apple-darwin" ;;
    darwin-arm64)  target="aarch64-apple-darwin" ;;
  esac

  if [[ -z "$target" ]]; then
    log_warn "No pre-built typst for ${os}-${arch}, trying cargo install..."
    install_typst_from_cargo
    return $?
  fi

  local tmpdir
  tmpdir=$(mktemp -d)
  local url="https://github.com/typst/typst/releases/latest/download/typst-${target}.tar.xz"
  if curl -fsSL "$url" -o "$tmpdir/typst.tar.xz" 2>/dev/null; then
    tar -xf "$tmpdir/typst.tar.xz" -C "$tmpdir"
    local bin
    bin=$(find "$tmpdir" -name "typst" -type f | head -1)
    if [[ -n "$bin" ]]; then
      sudo install -m 755 "$bin" "${TOOLS_INSTALL_DIR}/typst"
      log_success "typst installed"
    else
      log_warn "typst binary not found in archive"
    fi
  else
    log_warn "Failed to download typst, trying cargo install..."
    install_typst_from_cargo
  fi
  rm -rf "$tmpdir"
}

install_slidev() {
  # slidev is a dependency of sldr (presentation tool)
  if command_exists slidev; then
    log_success "slidev already installed"
    return 0
  fi

  log_info "Installing slidev (sli.dev)..."
  if command_exists bun; then
    bun install -g @slidev/cli 2>&1 | tail -3
  elif command_exists npm; then
    npm install -g @slidev/cli 2>&1 | tail -3
  else
    log_warn "Neither bun nor npm found, cannot install slidev"
    return 1
  fi

  local slidev_path=""
  if command_exists slidev; then
    slidev_path="$(command -v slidev)"
  elif [[ -x "$HOME/.bun/bin/slidev" ]]; then
    slidev_path="$HOME/.bun/bin/slidev"
  fi

  if [[ -n "$slidev_path" && "$slidev_path" != "${TOOLS_INSTALL_DIR}/slidev" ]]; then
    sudo install -m 755 "$slidev_path" "${TOOLS_INSTALL_DIR}/slidev"
    log_success "Installed slidev to ${TOOLS_INSTALL_DIR}/slidev"
  fi

  log_success "slidev installed"
}

install_all_agent_tools() {
  log_step "Installing agent tools"

  # External dependencies for agent tools
  install_typst
  install_slidev

  # Core tools (Rust) - tries pre-built GitHub release first, falls back to cargo
  # Multi-binary repos need the 3rd arg (package hint) for cargo fallback
  download_or_build_tool hstry hstry hstry-cli
  download_or_build_tool hstry-tui hstry hstry-tui
  download_or_build_tool agntz
  download_or_build_tool mmry mmry mmry-cli
  download_or_build_tool mmry-service mmry mmry-service
  download_or_build_tool tmpltr
  download_or_build_tool sldr sldr sldr-cli
  download_or_build_tool ignr

  # Core tools (Go) - tries pre-built GitHub release first, falls back to go install
  download_or_build_tool scrpr scrpr "" go
  download_or_build_tool sx sx "" go
}

select_agent_tools() {
  log_step "Agent Tools Selection"

  echo
  echo "Oqto can install agent tools:"
  echo
  echo -e "  ${BOLD}Core tools (recommended):${NC}"
  echo "    agntz   - Agent toolkit (file reservations, tool management)"
  echo "    mmry    - Memory storage and semantic search"
  echo "    scrpr   - Web content extraction"
  echo "    sx      - Web search via local SearXNG instance"
  echo
  echo -e "  ${BOLD}Additional tools:${NC}"
  echo "    tmpltr  - Document generation from templates (requires typst)"
  echo "    sldr    - Modular presentations (requires slidev/sli.dev)"
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

  # hstry is required for per-user chat history (systemd user service)
  download_or_build_tool hstry hstry hstry-cli

  if [[ "$INSTALL_MMRY" == "true" ]]; then
    download_or_build_tool mmry mmry mmry-cli
    download_or_build_tool mmry-service mmry mmry-service
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
#   - Runs under the backend user (current user in single-user, 'oqto' in multi-user)
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
    searxng_user="oqto"
    searxng_base="${OQTO_HOME}/.local/share/searxng"
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
# SearXNG settings - generated by Oqto setup.sh
# Local instance for agent web search via sx CLI

use_default_settings: true

general:
  debug: false
  instance_name: "Oqto SearXNG"

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

# sx configuration - generated by Oqto setup.sh
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
#   - Oqto creates per-session virtual keys automatically
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

  download_or_build_tool eavs

  if ! command_exists eavs; then
    log_error "EAVS installation failed"
    return 1
  fi

  # Install TypeScript adapters for model export (Pi, OpenCode, etc.)
  install_eavs_adapters

  log_success "EAVS installed: $(eavs --version 2>/dev/null | head -1)"
}

# Install eavs TypeScript adapters for 'eavs models export'.
# These live next to the binary so eavs can discover them automatically.
install_eavs_adapters() {
  local eavs_bin
  eavs_bin=$(command -v eavs 2>/dev/null) || return 0
  local eavs_dir
  eavs_dir=$(dirname "$eavs_bin")
  local adapters_dest="${eavs_dir}/adapters"

  # If adapters already exist next to binary (from release tarball), done
  if [[ -d "$adapters_dest" && -f "$adapters_dest/pi/adapter.ts" ]]; then
    log_info "EAVS adapters already installed"
    return 0
  fi

  # Fetch adapters from the eavs repo
  local version
  version=$(get_dep_version eavs)
  local tag="v${version:-main}"

  log_info "Installing EAVS adapters..."
  local tmpdir
  tmpdir=$(mktemp -d)

  if curl -fsSL "https://github.com/byteowlz/eavs/archive/refs/tags/${tag}.tar.gz" |
    tar xz -C "$tmpdir" --strip-components=1 "*/adapters" 2>/dev/null; then
    sudo mkdir -p "$adapters_dest"
    sudo cp -r "$tmpdir/adapters/"* "$adapters_dest/"
    log_success "EAVS adapters installed to $adapters_dest"
  else
    # Try main branch as fallback
    if curl -fsSL "https://github.com/byteowlz/eavs/archive/refs/heads/main.tar.gz" |
      tar xz -C "$tmpdir" --strip-components=1 "*/adapters" 2>/dev/null; then
      sudo mkdir -p "$adapters_dest"
      sudo cp -r "$tmpdir/adapters/"* "$adapters_dest/"
      log_success "EAVS adapters installed from main branch"
    else
      log_warn "Could not fetch EAVS adapters. 'eavs models export' will not work."
    fi
  fi

  rm -rf "$tmpdir"
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

  # In multi-user mode, also enable gnome-keyring for the oqto system user.
  # The EAVS service runs as oqto and needs D-Bus + gnome-keyring for the
  # keychain: config syntax and `eavs secret set` commands.
  if [[ "$SELECTED_USER_MODE" == "multi" ]] && id oqto &>/dev/null; then
    enable_keyring_for_octo_user
  fi
}

# Enable gnome-keyring-daemon for the oqto system user so that:
#   1. The EAVS system service (User=oqto) can resolve keychain: secrets at startup
#   2. Admins can run `sudo -u oqto dbus-run-session -- eavs secret set <name>`
# Requires linger so oqto's user-level systemd instance persists without a login.
enable_keyring_for_octo_user() {
  log_info "Enabling gnome-keyring for oqto user..."

  # Enable linger so oqto gets a persistent user-level systemd instance
  sudo loginctl enable-linger oqto 2>/dev/null || true

  # Enable the gnome-keyring socket for the oqto user
  if [[ -f /usr/lib/systemd/user/gnome-keyring-daemon.socket ]]; then
    sudo -u oqto systemctl --user enable gnome-keyring-daemon.socket 2>/dev/null || true
    sudo -u oqto systemctl --user start gnome-keyring-daemon.socket 2>/dev/null || true
    log_success "gnome-keyring-daemon.socket enabled for oqto user"
  fi
}

configure_eavs() {
  log_step "Configuring EAVS"

  # Determine config/data paths based on user mode:
  #   single-user: ~/.config/eavs/ (runs as installing user)
  #   multi-user:  ~oqto/.config/eavs/ (runs as oqto system user, same home)
  local eavs_config_dir eavs_data_dir eavs_env_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_config_dir="${XDG_CONFIG_HOME}/eavs"
    eavs_data_dir="${XDG_DATA_HOME:-$HOME/.local/share}/eavs"
    eavs_env_file="${eavs_config_dir}/env"
    mkdir -p "$eavs_config_dir" "$eavs_data_dir"
  else
    eavs_config_dir="${OQTO_HOME}/.config/eavs"
    eavs_data_dir="${OQTO_HOME}/.local/share/eavs"
    eavs_env_file="${eavs_config_dir}/env"
    sudo mkdir -p "$eavs_config_dir" "$eavs_data_dir"
  fi

  local eavs_config_file="${eavs_config_dir}/config.toml"

  # Clear env file on reconfigure to avoid duplicate keys
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    : >"${eavs_env_file}"
  else
    sudo bash -c ": > '${eavs_env_file}'"
  fi

  # Generate master key for oqto to create per-session virtual keys (reuse saved one)
  if [[ -z "${EAVS_MASTER_KEY:-}" ]]; then
    EAVS_MASTER_KEY=$(generate_secure_secret 32)
  else
    log_info "Using saved EAVS master key"
  fi

  echo
  echo "EAVS needs at least one LLM provider to route agent requests."
  echo "You can add more providers later by editing: $eavs_config_file"
  echo

  # Ensure the eavs model catalog is downloaded/up-to-date for model selection
  log_info "Updating model catalog from models.dev..."
  eavs models update >/dev/null 2>&1 || true

  # Collect provider configs (including model shortlists)
  local providers_toml=""
  local first_provider=""
  local has_any_provider="false"
  # Track configured providers for testing later
  CONFIGURED_PROVIDERS=""

  sanitize_provider_name() {
    local name="$1"
    name=$(echo "$name" | tr '[:space:]' '-' | tr -cd '[:alnum:]_-')
    echo "$name"
  }

  append_custom_provider() {
    local cp_name="$1"
    local cp_type="$2"
    local cp_base_url="$3"
    local cp_api_key="$4"
    local cp_deployment="$5"
    local cp_api_version="$6"
    local cp_aws_region="$7"
    local cp_gcp_project="$8"
    local cp_gcp_location="$9"
    local cp_test_model="${10}"
    local cp_allow_missing_env="${11:-false}"

    local cp_api_key_ref=""
    if [[ -n "$cp_api_key" ]]; then
      if [[ "$cp_api_key" == env:* ]]; then
        local env_name="${cp_api_key#env:}"
        local env_val="${!env_name:-}"
        if [[ -z "$env_val" && "$cp_allow_missing_env" != "true" ]]; then
          if [[ "$NONINTERACTIVE" != "true" ]]; then
            env_val=$(prompt_input "  ${cp_name} API key (env:${env_name})")
          fi
        fi
        if [[ -n "$env_val" ]]; then
          if [[ "$SELECTED_USER_MODE" == "single" ]]; then
            echo "${env_name}=${env_val}" >>"${eavs_env_file}"
          else
            echo "${env_name}=${env_val}" | sudo tee -a "${eavs_env_file}" >/dev/null
          fi
          cp_api_key_ref="env:${env_name}"
        else
          cp_api_key_ref="env:${env_name}"
          log_warn "Missing ${env_name} for provider '${cp_name}'"
        fi
      else
        local safe_name
        safe_name=$(echo "$cp_name" | tr '[:lower:]-' '[:upper:]_')
        local env_name="CUSTOM_${safe_name}_API_KEY"
        if [[ "$SELECTED_USER_MODE" == "single" ]]; then
          echo "${env_name}=${cp_api_key}" >>"${eavs_env_file}"
        else
          echo "${env_name}=${cp_api_key}" | sudo tee -a "${eavs_env_file}" >/dev/null
        fi
        cp_api_key_ref="env:${env_name}"
      fi
    fi

    providers_toml+="
[providers.${cp_name}]
"
    if [[ -n "$cp_type" ]]; then
      providers_toml+="type = \"${cp_type}\"\n"
    fi
    if [[ -n "$cp_base_url" ]]; then
      providers_toml+="base_url = \"${cp_base_url}\"\n"
    fi
    if [[ -n "$cp_api_key_ref" ]]; then
      providers_toml+="api_key = \"${cp_api_key_ref}\"\n"
    fi
    if [[ -n "$cp_deployment" ]]; then
      providers_toml+="deployment = \"${cp_deployment}\"\n"
    fi
    if [[ -n "$cp_api_version" ]]; then
      providers_toml+="api_version = \"${cp_api_version}\"\n"
    fi
    if [[ -n "$cp_aws_region" ]]; then
      providers_toml+="aws_region = \"${cp_aws_region}\"\n"
    fi
    if [[ -n "$cp_gcp_project" ]]; then
      providers_toml+="gcp_project = \"${cp_gcp_project}\"\n"
    fi
    if [[ -n "$cp_gcp_location" ]]; then
      providers_toml+="gcp_location = \"${cp_gcp_location}\"\n"
    fi
    if [[ -n "$cp_test_model" ]]; then
      providers_toml+="test_model = \"${cp_test_model}\"\n"
    fi

    has_any_provider="true"
    CONFIGURED_PROVIDERS="${CONFIGURED_PROVIDERS} ${cp_name}"
    if [[ -z "$first_provider" ]]; then
      first_provider="$cp_name"
    fi
  }

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
    local should_configure="false"

    if [[ -n "$existing_key" ]]; then
      log_info "Found $env_var_name in environment"
      if confirm "Configure $display_name (key found in env)?"; then
        should_configure="true"
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
          should_configure="true"
        fi
      fi
    fi

    if [[ "$should_configure" == "true" ]]; then
      # Provider base config
      providers_toml+="
[providers.${provider_name}]
type = \"${provider_type}\"
api_key = \"env:${env_var_name}\"
"
      # Let user pick models from the live eavs catalog.
      # Call directly (not in $()) so interactive TUI tools (gum/fzf) have
      # access to /dev/tty. The function sets _SELECT_MODELS_RESULT.
      _SELECT_MODELS_RESULT=""
      select_models_for_provider "$provider_name"
      providers_toml+="$_SELECT_MODELS_RESULT"

      has_any_provider="true"
      CONFIGURED_PROVIDERS="${CONFIGURED_PROVIDERS} ${provider_name}"
      if [[ -z "$first_provider" ]]; then
        first_provider="$provider_name"
      fi
    fi
  done

  # Add custom providers interactively
  if [[ "$NONINTERACTIVE" != "true" ]]; then
    while confirm "Add a custom provider (Azure/OpenAI-compatible/Foundry/etc.)?" "n"; do
      local raw_name
      raw_name=$(prompt_input "Provider name (used in config)" "foundry")
      local cp_name
      cp_name=$(sanitize_provider_name "$raw_name")
      if [[ -z "$cp_name" ]]; then
        log_warn "Provider name must include letters or numbers"
        continue
      fi
      if [[ " ${CONFIGURED_PROVIDERS} " =~ " ${cp_name} " ]]; then
        log_warn "Provider '${cp_name}' already configured"
        continue
      fi

      local cp_type
      cp_type=$(prompt_choice "Provider type" "microsoft-foundry" "azure" "openai-compatible" "foundry" "azure-foundry" "ai-foundry" "custom")
      if [[ "$cp_type" == "custom" ]]; then
        cp_type=$(prompt_input "Provider type (see eavs schema)" "openai-compatible")
      fi

      local default_env_name
      default_env_name=$(echo "$cp_name" | tr '[:lower:]-' '[:upper:]_')
      default_env_name="CUSTOM_${default_env_name}_API_KEY"

      local cp_api_key
      cp_api_key=$(prompt_input "API key (leave empty to set later; stored as env:${default_env_name})")
      if [[ -z "$cp_api_key" ]]; then
        cp_api_key="env:${default_env_name}"
      fi

      local cp_base_url
      cp_base_url=$(prompt_input "Base URL (leave empty for provider default)")

      local cp_deployment=""
      local cp_api_version=""
      if [[ "$cp_type" =~ ^(azure|microsoft-foundry|foundry|azure-foundry|ai-foundry)$ ]]; then
        cp_deployment=$(prompt_input "Deployment (Azure/Foundry, optional)")
        cp_api_version=$(prompt_input "API version (Azure/Foundry, optional)")
      fi

      local cp_test_model
      cp_test_model=$(prompt_input "Test model (optional, used for eavs setup test)")

      append_custom_provider "$cp_name" "$cp_type" "$cp_base_url" "$cp_api_key" "$cp_deployment" "$cp_api_version" "" "" "" "$cp_test_model" "true"
    done
  fi

  # Add custom providers from oqto.setup.toml (if any)
  if [[ ${#CUSTOM_PROVIDERS[@]} -gt 0 ]]; then
    for cp_key in "${CUSTOM_PROVIDERS[@]}"; do
      local cp_name
      cp_name=$(sanitize_provider_name "$cp_key")
      if [[ -z "$cp_name" ]]; then
        log_warn "Skipping custom provider with invalid name: ${cp_key}"
        continue
      fi
      if [[ " ${CONFIGURED_PROVIDERS} " =~ " ${cp_name} " ]]; then
        log_warn "Provider '${cp_name}' already configured"
        continue
      fi

      local cp_type="${CP_TYPE[$cp_key]}"
      local cp_base_url="${CP_BASE_URL[$cp_key]}"
      local cp_api_key="${CP_API_KEY[$cp_key]}"
      local cp_deployment="${CP_DEPLOYMENT[$cp_key]}"
      local cp_api_version="${CP_API_VERSION[$cp_key]}"
      local cp_aws_region="${CP_AWS_REGION[$cp_key]}"
      local cp_gcp_project="${CP_GCP_PROJECT[$cp_key]}"
      local cp_gcp_location="${CP_GCP_LOCATION[$cp_key]}"
      local cp_test_model="${CP_TEST_MODEL[$cp_key]}"

      append_custom_provider "$cp_name" "$cp_type" "$cp_base_url" "$cp_api_key" "$cp_deployment" "$cp_api_version" "$cp_aws_region" "$cp_gcp_project" "$cp_gcp_location" "$cp_test_model"
    done
  fi

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
  config_content=$(
    cat <<EOF
"\$schema" = "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/eavs/eavs.config.schema.json"

# EAVS Configuration - generated by Oqto setup.sh
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
    # Owned by oqto - same user that runs the eavs service
    sudo chown -R oqto:oqto "$eavs_config_dir" "$eavs_data_dir"
  fi

  log_success "EAVS config written to $eavs_config_file"
  if [[ -n "$first_provider" ]]; then
    log_success "Default provider: $first_provider"
  fi
}

# ==============================================================================
# Dynamic Model Selection from EAVS Catalog
# ==============================================================================
# Queries the live eavs model catalog (models.dev) and lets users pick models
# interactively via fuzzy multi-select. Falls back gracefully:
#   gum filter (best UX) > fzf (good) > numbered list (basic)
# In non-interactive mode, auto-selects the top N newest models.
#
# The catalog is sorted by release date (newest first) by eavs, so the
# pre-selected defaults are always the latest models.

# How many models to pre-select per provider in non-interactive/default mode
DEFAULT_MODEL_COUNT=5

select_models_for_provider() {
  local provider="$1"

  # Result is returned via _SELECT_MODELS_RESULT (not stdout) so that
  # the function runs in the current shell and TUI tools have /dev/tty.
  _SELECT_MODELS_RESULT=""

  # Check if eavs supports the 'models' subcommand (>= 0.5.4)
  if ! eavs models list "$provider" --json >/dev/null 2>&1; then
    log_warn "eavs model catalog not available (upgrade eavs to >= 0.5.4 for model selection)"
    log_info "Skipping model selection for $provider -- all catalog models will be available"
    return
  fi

  # Query the eavs model catalog for this provider
  local catalog_json
  catalog_json=$(eavs models list "$provider" --json 2>/dev/null) || true

  if [[ -z "$catalog_json" || "$catalog_json" == "[]" || "$catalog_json" == "null" ]]; then
    log_warn "No models found in catalog for $provider. Skipping model selection."
    return
  fi

  # Build tab-separated display data using jq
  # Each line: "model_id\tname\t$in/$out\tctx\treasoning\trelease_date"
  local model_lines
  model_lines=$(echo "$catalog_json" | jq -r '.[] |
    def fmt_ctx: .limit.context // 0 |
      if . >= 1000000 then "\(./1000000 | floor)M"
      elif . >= 1000 then "\(./1000 | floor)K"
      else tostring end;
    [
      .id,
      (.name // .id),
      "$\(.cost.input // 0)/$\(.cost.output // 0)",
      fmt_ctx,
      (if .reasoning then "R" else " " end),
      (.release_date // "")[:10]
    ] | @tsv
  ' 2>/dev/null) || true

  if [[ -z "$model_lines" ]]; then
    log_warn "Failed to parse model catalog for $provider"
    return
  fi

  local total_count
  total_count=$(echo "$model_lines" | wc -l)

  # Build formatted display lines for the picker (tab-separated data -> columns)
  local display_lines
  display_lines=$(echo "$model_lines" | awk -F'\t' '{
    printf "%-45s  %s  %-12s  ctx=%-6s  %s\n", $1, $5, $3, $4, $6
  }')

  # Get the top N model IDs for pre-selection
  local default_ids
  default_ids=$(echo "$model_lines" | head -n "$DEFAULT_MODEL_COUNT" | cut -f1)
  local default_csv
  default_csv=$(echo "$default_ids" | paste -sd',' -)

  echo
  echo "  Select models for $provider ($total_count available, newest first):"
  echo "  [R]=reasoning  Costs per 1M tokens  Defaults: top $DEFAULT_MODEL_COUNT newest"
  echo

  local selected_ids=""

  if [[ "$NONINTERACTIVE" == "true" ]]; then
    # Non-interactive: just use the top N
    selected_ids="$default_ids"
    log_info "Auto-selected top $DEFAULT_MODEL_COUNT models for $provider"
  elif command -v gum >/dev/null 2>&1; then
    # gum filter: fuzzy search + multi-select (best UX)
    # Write display lines to temp file to avoid pipe/subshell TTY issues
    local tmpfile selected_tmpfile
    tmpfile=$(mktemp)
    selected_tmpfile=$(mktemp)
    echo "$display_lines" > "$tmpfile"

    # Use gum with file redirection to avoid subshell TTY issues.
    # Note: Pre-selection is skipped because --selected CSV parsing breaks
    # when display lines contain commas. Models are already sorted newest-first.
    gum filter --no-limit \
      --header="Select models for $provider (tab=toggle, enter=confirm, ctrl+c=cancel)" \
      --placeholder="Type to filter... (top $DEFAULT_MODEL_COUNT are recommended)" \
      --height=20 < "$tmpfile" > "$selected_tmpfile" 2>/dev/null || true

    selected_ids=$(awk '{print $1}' "$selected_tmpfile" 2>/dev/null) || true
    rm -f "$tmpfile" "$selected_tmpfile"
  elif command -v fzf >/dev/null 2>&1; then
    # fzf: fuzzy search + multi-select (good fallback)
    # Use temp file to avoid subshell TTY issues
    local tmpfile
    tmpfile=$(mktemp)
    echo "$display_lines" > "$tmpfile"

    local selected_tmpfile
    selected_tmpfile=$(mktemp)

    fzf --multi \
      --header="Select models for $provider (tab=toggle, enter=confirm)" \
      --height=20 \
      --reverse < "$tmpfile" > "$selected_tmpfile" 2>/dev/null || true

    selected_ids=$(awk '{print $1}' "$selected_tmpfile" 2>/dev/null) || true
    rm -f "$tmpfile" "$selected_tmpfile"
  fi

  # Fallback: simple numbered list if no TUI tool or nothing selected
  if [[ -z "$selected_ids" && "$NONINTERACTIVE" != "true" ]]; then
    echo "  Available models:"
    local i=1
    while IFS=$'\t' read -r mid name cost ctx reasoning rel; do
      local marker=" "
      if echo "$default_ids" | grep -qx "$mid"; then
        marker="*"
      fi
      printf "  %s %2d) %-40s  %s  %-12s  ctx=%-6s  %s\n" "$marker" "$i" "$mid" "$reasoning" "$cost" "$ctx" "$rel"
      i=$((i + 1))
    done <<<"$model_lines"
    echo
    echo "  Enter model numbers to select (comma/space separated, * = pre-selected)."
    echo "  Press Enter to accept defaults (top $DEFAULT_MODEL_COUNT)."
    local selection
    read -r -p "  Selection: " selection

    if [[ -z "$selection" ]]; then
      # Accept defaults
      selected_ids="$default_ids"
    else
      # Parse comma/space separated numbers
      selected_ids=""
      local nums
      nums=$(echo "$selection" | tr ',' ' ')
      for num in $nums; do
        local sel_id
        sel_id=$(echo "$model_lines" | sed -n "${num}p" | cut -f1)
        if [[ -n "$sel_id" ]]; then
          selected_ids+="$sel_id"$'\n'
        fi
      done
    fi
  fi

  if [[ -z "$selected_ids" ]]; then
    log_warn "No models selected for $provider"
    return
  fi

  # Convert selected model IDs to TOML shortlist entries using jq
  local toml_output=""
  while IFS= read -r model_id; do
    [[ -z "$model_id" ]] && continue
    # Look up full model data from catalog JSON and format as TOML
    local model_toml
    model_toml=$(echo "$catalog_json" | jq -r --arg id "$model_id" --arg prov "$provider" '
      .[] | select(.id == $id) |
      # Filter input modalities to text/image for Pi compatibility
      ([.modalities.input[]? | select(. == "text" or . == "image")] | if length == 0 then ["text"] else . end) as $input |
      "
[[providers.\($prov).models]]
id = \"\(.id)\"
name = \"\(.name // .id)\"
reasoning = \(if .reasoning then "true" else "false" end)
input = [\($input | map("\"" + . + "\"") | join(", "))]
context_window = \(.limit.context // 128000)
max_tokens = \(.limit.output // 8192)
cost = { input = \(.cost.input // 0), output = \(.cost.output // 0), cache_read = \(.cost.cache_read // 0) }"
    ' 2>/dev/null) || true
    toml_output+="$model_toml"
  done <<<"$selected_ids"

  local selected_count
  selected_count=$(echo "$selected_ids" | grep -c '.' || echo 0)
  log_success "Selected $selected_count models for $provider"
  _SELECT_MODELS_RESULT="$toml_output"
}

# ==============================================================================
# EAVS Provider Testing
# ==============================================================================
# Tests each configured provider by making a real API call via `eavs setup test`.
# This validates that API keys are correct and providers are reachable.

test_eavs_providers() {
  log_step "Testing LLM provider connections"

  local eavs_config_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_config_file="${XDG_CONFIG_HOME}/eavs/config.toml"
  else
    eavs_config_file="${OQTO_HOME}/.config/eavs/config.toml"
  fi

  # Resolve env file for the test command
  local eavs_env_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_env_file="${XDG_CONFIG_HOME}/eavs/env"
  else
    eavs_env_file="${OQTO_HOME}/.config/eavs/env"
  fi

  if [[ -z "${CONFIGURED_PROVIDERS// }" ]]; then
    log_warn "No providers configured. Skipping provider tests."
    return 0
  fi

  read_env_file() {
    if [[ "$SELECTED_USER_MODE" == "single" ]]; then
      cat "$eavs_env_file" 2>/dev/null || true
    else
      sudo cat "$eavs_env_file" 2>/dev/null || true
    fi
  }

  get_provider_api_key_ref() {
    local provider_name="$1"
    awk -v p="$provider_name" '
      $0 ~ "^\\[providers\\."p"\\]" {in=1; next}
      $0 ~ "^\\[providers\\." && $0 !~ "^\\[providers\\."p"\\]" {in=0}
      in && $0 ~ "^api_key" {
        sub(/^[^=]*= */, "", $0)
        gsub(/\"/, "", $0)
        print $0
        exit
      }
    ' "$eavs_config_file" 2>/dev/null
  }

  get_env_value() {
    local env_name="$1"
    local env_val="${!env_name:-}"
    if [[ -n "$env_val" ]]; then
      echo "$env_val"
      return
    fi
    read_env_file | sed -n "s/^${env_name}=//p" | head -1
  }

  local any_success="false"
  local any_failure="false"
  local summary_lines=()

  for provider in $CONFIGURED_PROVIDERS; do
    [[ -z "$provider" ]] && continue
    echo -n "  Testing ${provider}... "

    local api_key_ref
    api_key_ref=$(get_provider_api_key_ref "$provider")
    if [[ "$api_key_ref" == env:* ]]; then
      local env_name="${api_key_ref#env:}"
      local env_val
      env_val=$(get_env_value "$env_name")
      if [[ -z "$env_val" ]]; then
        echo -e "${YELLOW}SKIPPED${NC}"
        echo "    Missing env var ${env_name} for ${provider}"
        summary_lines+=("${provider}|SKIPPED|Missing env var ${env_name}")
        any_failure="true"
        continue
      fi
    fi

    # Source the env file so eavs setup test can resolve env: keys.
    # Redirect stdin from /dev/null so eavs doesn't try to prompt for input.
    local test_result
    if [[ "$SELECTED_USER_MODE" == "single" ]]; then
      test_result=$(
        set -a
        source "$eavs_env_file" 2>/dev/null
        set +a
        eavs setup test "$provider" --config "$eavs_config_file" --format json </dev/null 2>&1
      ) || true
    else
      # Ensure oqto can read the env file and config.
      sudo chown -R oqto:oqto "$(dirname "$eavs_config_file")" 2>/dev/null
      # Source env file in the current shell first, then pass vars via sudo env.
      local env_args=""
      if [[ -f "$eavs_env_file" ]]; then
        while IFS='=' read -r key value; do
          [[ -z "$key" || "$key" == \#* ]] && continue
          env_args+="$key=$value "
        done < <(sudo cat "$eavs_env_file" 2>/dev/null)
      fi
      test_result=$(sudo -u oqto env $env_args \
        eavs setup test "$provider" --config "$eavs_config_file" --format json </dev/null 2>&1) || true
    fi

    if echo "$test_result" | grep -qE '"success"[[:space:]]*:[[:space:]]*true|test successful'; then
      echo -e "${GREEN}OK${NC}"
      summary_lines+=("${provider}|OK|")
      any_success="true"
    else
      echo -e "${RED}FAILED${NC}"
      # Show a brief error hint
      local err_hint
      err_hint=$(echo "$test_result" | grep -i "error\|unauthorized\|invalid\|403\|401" | head -1)
      if [[ -n "$err_hint" ]]; then
        echo "    $err_hint"
      fi
      summary_lines+=("${provider}|FAILED|${err_hint}")
      any_failure="true"
    fi
  done

  echo
  log_info "Provider test summary:"
  for entry in "${summary_lines[@]}"; do
    IFS='|' read -r provider status detail <<<"$entry"
    printf "  %-20s %s\n" "$provider" "$status"
    if [[ -n "$detail" ]]; then
      echo "    ${detail}"
    fi
  done

  if [[ "$any_success" == "true" ]]; then
    log_success "At least one provider is working"
  fi
  if [[ "$any_failure" == "true" ]]; then
    log_warn "Some providers failed. You can fix API keys later in the eavs config."
    if [[ "$any_success" != "true" ]]; then
      log_warn "No working providers! Agents will not be able to use any LLM."
      log_info "Fix provider config: edit $(
        if [[ "$SELECTED_USER_MODE" == "single" ]]; then
          echo "${XDG_CONFIG_HOME}/eavs/config.toml"
        else
          echo "${OQTO_HOME}/.config/eavs/config.toml"
        fi
      )"
    fi
  fi
}

# ==============================================================================
# EAVS models.json Generation
# ==============================================================================
# Uses `eavs models export pi` to generate Pi-compatible models.json.
# The export command reads the eavs config, resolves model shortlists
# (or full catalog), and outputs the correct Pi format natively.
#
# In single-user mode: writes to ~/.pi/agent/models.json
# In multi-user mode: the oqto backend handles this at user creation time
#   (via provision_eavs_for_user in admin.rs), but we also generate a
#   template for the installing admin user.

generate_eavs_models_json() {
  log_step "Generating models.json from EAVS"

  local eavs_url="http://127.0.0.1:${EAVS_PORT}"
  local eavs_config_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    eavs_config_file="${XDG_CONFIG_HOME}/eavs/config.toml"
  else
    eavs_config_file="${OQTO_HOME}/.config/eavs/config.toml"
  fi

  # Check that eavs supports the export command (>= 0.5.5)
  if ! eavs models export --help >/dev/null 2>&1; then
    log_warn "eavs does not support 'models export' (upgrade to >= 0.5.5)"
    log_info "Skipping models.json generation. Update eavs and re-run setup."
    return 0
  fi

  # Generate Pi models.json via native eavs export
  # Use --merge if a models.json already exists to preserve non-eavs providers
  local pi_models_file
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    pi_models_file="$HOME/.pi/agent/models.json"
  else
    pi_models_file="${OQTO_DATA_DIR:-$HOME/.local/share/oqto}/models.json.template"
  fi

  local merge_flag=""
  if [[ -f "$pi_models_file" ]]; then
    merge_flag="--merge $pi_models_file"
    log_info "Merging into existing $pi_models_file (preserving non-eavs providers)"
  fi

  local models_json=""
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    # shellcheck disable=SC2086
    models_json=$(eavs models export pi \
      --base-url "$eavs_url" \
      --config "$eavs_config_file" \
      $merge_flag 2>/dev/null) || true
  else
    # shellcheck disable=SC2086
    models_json=$(sudo -u oqto eavs models export pi \
      --base-url "$eavs_url" \
      --config "$eavs_config_file" \
      $merge_flag 2>/dev/null) || true
  fi

  if [[ -z "$models_json" || "$models_json" == '{"providers":{}}' ]]; then
    log_warn "No providers with Pi-compatible APIs found. Skipping models.json."
    return 0
  fi

  # Write models.json
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    local pi_agent_dir="$HOME/.pi/agent"
    mkdir -p "$pi_agent_dir"
    echo "$models_json" >"${pi_agent_dir}/models.json"
    log_success "Wrote models.json to ${pi_agent_dir}/models.json"

    # Also create eavs.env for Pi to use (with a virtual key)
    provision_eavs_user_key "$(whoami)" "$HOME"
  else
    # In multi-user mode, write a template that the oqto backend will
    # use when provisioning new users. Also set up the admin user.
    local octo_data="${OQTO_DATA_DIR:-$HOME/.local/share/oqto}"
    mkdir -p "$octo_data"
    echo "$models_json" >"${octo_data}/models.json.template"
    log_success "Wrote models.json template to ${octo_data}/models.json.template"
    log_info "The oqto backend will generate per-user models.json on user creation."
  fi

  # Count total models and providers using jq (available on all target systems)
  local model_count provider_count
  model_count=$(echo "$models_json" | jq '[.providers[].models | length] | add // 0' 2>/dev/null || echo "?")
  provider_count=$(echo "$models_json" | jq '.providers | length' 2>/dev/null || echo "?")

  log_success "Models available: $model_count across $provider_count provider(s)"
}

# ==============================================================================
# EAVS User Key Provisioning
# ==============================================================================
# Creates a virtual API key for a user and writes eavs.env so Pi can
# authenticate against the eavs proxy.

provision_eavs_user_key() {
  local username="$1"
  local user_home="$2"

  local eavs_url="http://127.0.0.1:${EAVS_PORT}"

  # Create virtual key via eavs API
  local key_response
  key_response=$(curl -sf -X POST "${eavs_url}/admin/keys" \
    -H "Authorization: Bearer ${EAVS_MASTER_KEY}" \
    -H "Content-Type: application/json" \
    -d "{
      \"name\": \"oqto-user-${username}\",
      \"permissions\": {
        \"rpm_limit\": 120,
        \"max_budget_usd\": 500.0
      }
    }" 2>&1)

  if [[ -z "$key_response" ]]; then
    log_warn "Failed to create EAVS virtual key for ${username}"
    log_info "Users can still use EAVS with the master key for now."
    # Fall back to master key
    local octo_config_dir="${user_home}/.config/oqto"
    mkdir -p "$octo_config_dir"
    cat >"${octo_config_dir}/eavs.env" <<EOF
EAVS_API_KEY=${EAVS_MASTER_KEY}
EAVS_URL=${eavs_url}
EOF
    chmod 600 "${octo_config_dir}/eavs.env"
    return
  fi

  local api_key
  api_key=$(echo "$key_response" | jq -r '.key // empty' 2>/dev/null || echo "")

  if [[ -z "$api_key" ]]; then
    log_warn "Could not parse EAVS key response. Using master key as fallback."
    api_key="$EAVS_MASTER_KEY"
  fi

  local octo_config_dir="${user_home}/.config/oqto"
  mkdir -p "$octo_config_dir"
  cat >"${octo_config_dir}/eavs.env" <<EOF
EAVS_API_KEY=${api_key}
EAVS_URL=${eavs_url}
EOF
  chmod 600 "${octo_config_dir}/eavs.env"
  log_success "EAVS key provisioned for ${username}"
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
    # Multi-user: system service, runs as oqto user alongside the backend.
    # EAVS config lives in oqto's home (~oqto/.config/eavs/) so XDG just works.
    #
    # The DBUS_SESSION_BUS_ADDRESS and XDG_RUNTIME_DIR environment variables
    # give the service access to the oqto user's D-Bus session bus, which is
    # needed for gnome-keyring (the keychain: config syntax). This requires
    # linger to be enabled for oqto (done in enable_keyring_for_octo_user).
    local octo_uid
    octo_uid=$(id -u oqto)

    sudo tee /etc/systemd/system/eavs.service >/dev/null <<EOF
[Unit]
Description=EAVS LLM Proxy
After=network.target
Before=oqto.service

[Service]
Type=simple
User=oqto
Group=oqto
WorkingDirectory=${OQTO_HOME}
Environment=HOME=${OQTO_HOME}
Environment=XDG_CONFIG_HOME=${OQTO_HOME}/.config
Environment=XDG_DATA_HOME=${OQTO_HOME}/.local/share
Environment=XDG_STATE_HOME=${OQTO_HOME}/.local/state
Environment=XDG_RUNTIME_DIR=/run/user/${octo_uid}
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${octo_uid}/bus
EnvironmentFile=-${OQTO_HOME}/.config/eavs/env
ExecStartPre=+/bin/bash -c 'mkdir -p /run/user/${octo_uid} && chown oqto:oqto /run/user/${octo_uid} && chmod 700 /run/user/${octo_uid}'
ExecStart=/usr/local/bin/eavs serve
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=full
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

    sudo systemctl daemon-reload
    sudo systemctl enable eavs
    sudo systemctl start eavs
    log_success "EAVS started (system service, user=oqto, port ${EAVS_PORT})"
  fi
}

# ==============================================================================
# Production Mode Setup
# ==============================================================================

select_deployment_mode() {
  log_step "Deployment Mode Selection"

  echo
  echo "Oqto can be deployed in two modes:"
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
    OQTO_DEV_MODE="true"
    log_info "Development mode selected"
    ;;
  "Production")
    PRODUCTION_MODE="true"
    OQTO_DEV_MODE="false"
    log_info "Production mode selected"
    setup_production_mode
    ;;
  esac
}

setup_production_mode() {
  log_step "Production Mode Configuration"

  # Generate JWT secret (reuse saved one if available)
  echo
  if [[ -n "${JWT_SECRET:-}" ]]; then
    log_info "Using saved JWT secret"
  else
    log_info "Generating secure JWT secret..."
    JWT_SECRET=$(generate_secure_secret 64)
    log_success "JWT secret generated (64 characters)"
  fi

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
  echo "Create an administrator account to manage Oqto."
  echo "This user will be able to:"
  echo "  - Access the admin dashboard"
  echo "  - Create invite codes for new users"
  echo "  - Manage sessions and users"
  echo

  # Username
  ADMIN_USERNAME=$(prompt_input "Admin username" "${ADMIN_USERNAME:-admin}")

  # Email
  ADMIN_EMAIL=$(prompt_input "Admin email" "${ADMIN_EMAIL:-admin@localhost}")

  # Password is prompted later in create_admin_user_db when actually needed.
  # We never persist plaintext passwords to disk.

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

  if [[ -n "$OQTO_SETUP_CADDY" ]]; then
    SETUP_CADDY="$OQTO_SETUP_CADDY"
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
  echo "  - oqto.example.com"
  echo "  - agents.mycompany.io"
  echo "  - localhost (for local testing without TLS)"
  echo

  if [[ -n "$OQTO_DOMAIN" ]]; then
    DOMAIN="$OQTO_DOMAIN"
  else
    DOMAIN=$(prompt_input "Domain name" "localhost")
  fi

  # Strip protocol prefix if user included it
  DOMAIN="${DOMAIN#https://}"
  DOMAIN="${DOMAIN#http://}"
  # Strip trailing slash
  DOMAIN="${DOMAIN%/}"

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

  # Determine ports and paths
  local backend_port="8080"
  local frontend_dir="/var/www/oqto"

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
# Oqto Caddyfile - Local Development
# Generated by setup.sh on $(date)

:80 {
    # Backend API (all routes are under /api on the backend)
    handle /api/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Frontend (static files with SPA fallback)
    handle {
        root * ${frontend_dir}
        try_files {path} /index.html
        file_server
    }
    
    log {
        output file /var/log/caddy/oqto.log
    }
}
EOF
  else
    # Production - with TLS
    sudo tee "$caddyfile" >/dev/null <<EOF
# Oqto Caddyfile - Production
# Generated by setup.sh on $(date)
# Domain: ${DOMAIN}

${DOMAIN} {
    # Backend API (all routes are under /api on the backend)
    handle /api/* {
        reverse_proxy localhost:${backend_port}
    }
    
    # Frontend (static files with SPA fallback)
    handle {
        root * ${frontend_dir}
        try_files {path} /index.html
        file_server
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
        output file /var/log/caddy/oqto.log
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
        # Use restart (not start) to pick up the new Caddyfile
        # start is a no-op if caddy is already running with the default config
        sudo systemctl restart caddy
        log_success "Caddy service started with Oqto config"
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

# ==============================================================================
# Update Mode
# ==============================================================================
# Pulls latest code, rebuilds everything, deploys, copies config, restarts.

update_octo() {
  log_step "Updating Oqto"

  cd "$SCRIPT_DIR"

  # Pull latest code
  log_info "Pulling latest code..."
  if git pull --ff-only; then
    log_success "Code updated"
  else
    log_warn "git pull --ff-only failed, trying rebase..."
    git pull --rebase || {
      log_error "Failed to pull latest code. Resolve conflicts and retry."
      return 1
    }
    log_success "Code updated (rebased)"
  fi

  # Upgrade external tools to versions in dependencies.toml
  update_tools

  # Rebuild Oqto (backend + frontend + deploy)
  build_octo

  # Copy config to service user (multi-user mode uses /var/lib/oqto)
  if [[ -f "/var/lib/oqto/.config/oqto/config.toml" ]]; then
    log_info "Syncing config to service user..."
    local octo_config_home="/var/lib/oqto/.config/oqto"
    sudo cp "${XDG_CONFIG_HOME:-$HOME/.config}/oqto/config.toml" "${octo_config_home}/config.toml"
    if [[ -f "${XDG_CONFIG_HOME:-$HOME/.config}/oqto/env" ]]; then
      sudo cp "${XDG_CONFIG_HOME:-$HOME/.config}/oqto/env" "${octo_config_home}/env"
      sudo chmod 600 "${octo_config_home}/env"
    fi
    sudo chown -R oqto:oqto "$octo_config_home"
    log_success "Config synced"
  fi

  # Regenerate models.json (picks up new eavs providers/models)
  update_models_json

  # Restart services
  log_info "Restarting services..."
  if sudo systemctl is-active --quiet eavs 2>/dev/null; then
    sudo systemctl restart eavs
    log_success "eavs restarted"
  fi
  if sudo systemctl is-active --quiet oqto 2>/dev/null; then
    sudo systemctl restart oqto
    log_success "oqto service restarted"
  elif systemctl --user is-active --quiet oqto 2>/dev/null; then
    systemctl --user restart oqto
    log_success "oqto service restarted"
  fi

  if sudo systemctl is-active --quiet caddy 2>/dev/null; then
    sudo systemctl restart caddy
    log_success "caddy restarted"
  fi

  # Quick health check
  sleep 2
  if curl -sf http://localhost:8080/api/health >/dev/null 2>&1; then
    log_success "Backend is healthy"
  else
    log_warn "Backend health check failed - check logs with: sudo journalctl -u oqto -n 20"
  fi

  log_success "Update complete!"
}

# Upgrade external tools to the versions tracked in dependencies.toml.
# Only upgrades tools that are already installed (doesn't install new ones).
# Skips tools already at the target version.
update_tools() {
  log_step "Upgrading external tools"

  # Tools that setup.sh manages (binary name -> repo name)
  local -A TOOLS=(
    [eavs]=eavs
    [hstry]=hstry
    [mmry]=mmry
    [trx]=trx
    [agntz]=agntz
    [mailz]=mailz
    [sx]=sx
    [scrpr]=scrpr
    [tmpltr]=tmpltr
    [ignr]=ignr
  )

  local upgraded=0

  for tool in "${!TOOLS[@]}"; do
    # Skip tools not currently installed
    if ! command_exists "$tool"; then
      continue
    fi

    local repo="${TOOLS[$tool]}"
    local target_version
    target_version=$(get_dep_version "$repo")
    [[ -z "$target_version" || "$target_version" == "latest" ]] && continue

    # Get currently installed version
    local current_version
    current_version=$("$tool" --version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "unknown")

    if [[ "$current_version" == "$target_version" ]]; then
      log_info "$tool $current_version (up to date)"
      continue
    fi

    log_info "Upgrading $tool: $current_version -> $target_version"
    download_or_build_tool "$tool" "$repo"
    ((upgraded++)) || true
  done

  # Special: eavs adapters (must be installed alongside the binary)
  if command_exists eavs; then
    install_eavs_adapters
  fi

  if ((upgraded > 0)); then
    log_success "Upgraded $upgraded tool(s)"
  else
    log_info "All tools up to date"
  fi
}

# Regenerate models.json without interactive prompts.
# Used by --update to pick up new models/providers from eavs config changes.
update_models_json() {
  # Need eavs with export support
  if ! command_exists eavs || ! eavs models export --help >/dev/null 2>&1; then
    return 0
  fi

  local eavs_config_file
  local eavs_url="http://127.0.0.1:${EAVS_PORT:-3033}"

  # Detect mode from existing config
  if [[ -f "/var/lib/oqto/.config/oqto/config.toml" ]]; then
    # Multi-user
    eavs_config_file="/var/lib/oqto/.config/eavs/config.toml"
    if [[ ! -f "$eavs_config_file" ]]; then
      eavs_config_file="${XDG_CONFIG_HOME:-$HOME/.config}/eavs/config.toml"
    fi

    local pi_models_file="$HOME/.pi/agent/models.json"
    local merge_flag=""
    if [[ -f "$pi_models_file" ]]; then
      merge_flag="--merge $pi_models_file"
    fi

    local models_json
    # shellcheck disable=SC2086
    models_json=$(eavs models export pi \
      --base-url "$eavs_url" \
      --config "$eavs_config_file" \
      $merge_flag 2>/dev/null) || true

    if [[ -n "$models_json" && "$models_json" != '{"providers":{}}' ]]; then
      mkdir -p "$HOME/.pi/agent"
      echo "$models_json" >"$HOME/.pi/agent/models.json"
      local count
      count=$(echo "$models_json" | jq '[.providers[].models | length] | add // 0' 2>/dev/null || echo "?")
      log_success "models.json updated ($count models)"
    fi
  else
    # Single-user
    eavs_config_file="${XDG_CONFIG_HOME:-$HOME/.config}/eavs/config.toml"
    local pi_models_file="$HOME/.pi/agent/models.json"
    local merge_flag=""
    if [[ -f "$pi_models_file" ]]; then
      merge_flag="--merge $pi_models_file"
    fi

    local models_json
    # shellcheck disable=SC2086
    models_json=$(eavs models export pi \
      --base-url "$eavs_url" \
      --config "$eavs_config_file" \
      $merge_flag 2>/dev/null) || true

    if [[ -n "$models_json" && "$models_json" != '{"providers":{}}' ]]; then
      mkdir -p "$HOME/.pi/agent"
      echo "$models_json" >"$HOME/.pi/agent/models.json"
      local count
      count=$(echo "$models_json" | jq '[.providers[].models | length] | add // 0' 2>/dev/null || echo "?")
      log_success "models.json updated ($count models)"
    fi
  fi
}

build_octo() {
  log_step "Building Oqto components"

  cd "$SCRIPT_DIR"

  # Clean up stale directories from octo->oqto rename that confuse workspace
  rm -rf backend/crates/octo-browserd backend/crates/octo-browser backend/crates/octo 2>/dev/null || true

  # Build backend (includes oqto, oqto-runner, oqto-sandbox, pi-bridge binaries)
  log_info "Building backend..."
  if ! (cd backend && cargo build --release); then
    log_error "Backend build failed"
    return 1
  fi
  log_success "Backend built"

  # Build fileserver (oqto-files crate in workspace)
  log_info "Building fileserver..."
  if ! (cd backend && cargo build --release -p oqto-files --bin oqto-files); then
    log_error "Fileserver build failed"
    return 1
  fi
  log_success "Fileserver built"

  # Build frontend
  log_info "Installing frontend dependencies..."
  (cd frontend && bun install)
  log_info "Building frontend..."
  if ! (cd frontend && bun run build); then
    log_error "Frontend build failed"
    return 1
  fi
  log_success "Frontend built"

  # Build and install agent browser daemon
  log_info "Building agent browser daemon..."
  local browserd_dir="$SCRIPT_DIR/backend/crates/oqto-browserd"
  if [[ -d "$browserd_dir" ]]; then
    (cd "$browserd_dir" && bun install && bun run build)
    local browserd_deploy="/usr/local/lib/oqto-browserd"
    sudo mkdir -p "$browserd_deploy/bin" "$browserd_deploy/dist"
    sudo cp "$browserd_dir/bin/oqto-browserd.js" "$browserd_deploy/bin/"
    sudo cp "$browserd_dir/dist/"*.js "$browserd_deploy/dist/"
    sudo cp "$browserd_dir/package.json" "$browserd_deploy/"
    (cd "$browserd_deploy" && sudo bun install --production)
    # Install Playwright browsers to a shared location accessible by all users
    local pw_browsers="/usr/local/share/playwright-browsers"
    sudo mkdir -p "$pw_browsers"
    sudo chmod 755 "$pw_browsers"
    log_info "Installing Playwright chromium to $pw_browsers..."
    sudo PLAYWRIGHT_BROWSERS_PATH="$pw_browsers" npx --yes playwright install --with-deps chromium 2>/dev/null ||
      sudo PLAYWRIGHT_BROWSERS_PATH="$pw_browsers" bunx playwright install --with-deps chromium 2>/dev/null ||
      log_warn "Playwright browser install failed - agent browser may not work"
    log_success "Agent browser daemon installed to $browserd_deploy"
  else
    log_warn "oqto-browserd not found, skipping agent browser setup"
  fi

  # Install binaries to /usr/local/bin (globally accessible)
  log_info "Installing binaries to ${TOOLS_INSTALL_DIR}..."

  local release_dir="$SCRIPT_DIR/backend/target/release"
  for bin in oqto oqtoctl oqto-runner pi-bridge oqto-sandbox oqto-setup oqto-usermgr; do
    if [[ -f "${release_dir}/${bin}" ]]; then
      sudo install -m 755 "${release_dir}/${bin}" "${TOOLS_INSTALL_DIR}/${bin}"
      # Remove stale copies from ~/.cargo/bin to avoid PATH precedence issues
      if [[ -f "$HOME/.cargo/bin/${bin}" ]]; then
        rm -f "$HOME/.cargo/bin/${bin}"
        hash -d "$bin" 2>/dev/null || true
        log_info "Removed stale ${bin} from ~/.cargo/bin"
      fi
      log_success "${bin} installed"
    fi
  done

  if [[ -f "$SCRIPT_DIR/backend/target/release/oqto-files" ]]; then
    sudo install -m 755 "$SCRIPT_DIR/backend/target/release/oqto-files" "${TOOLS_INSTALL_DIR}/oqto-files"
    log_success "oqto-files installed"
  fi

  log_success "Binaries installed to ${TOOLS_INSTALL_DIR}"

  # Deploy frontend static files
  local frontend_dist="$SCRIPT_DIR/frontend/dist"
  local frontend_deploy="/var/www/oqto"
  if [[ -d "$frontend_dist" ]]; then
    log_info "Deploying frontend to ${frontend_deploy}..."
    sudo mkdir -p "$frontend_deploy"
    sudo rsync -a --delete "$frontend_dist/" "$frontend_deploy/"
    sudo chown -R root:root "$frontend_deploy"
    log_success "Frontend deployed to ${frontend_deploy}"
  else
    log_warn "Frontend dist not found, skipping deployment"
  fi

  # Restart running services so they pick up the new binaries
  if sudo systemctl is-active --quiet oqto-usermgr 2>/dev/null; then
    sudo systemctl restart oqto-usermgr
    log_success "oqto-usermgr restarted"
  fi
  if sudo systemctl is-active --quiet oqto 2>/dev/null; then
    sudo systemctl restart oqto
    log_success "oqto restarted with new binary"
  elif systemctl --user is-active --quiet oqto 2>/dev/null; then
    systemctl --user restart oqto
    log_success "oqto restarted with new binary"
  fi
  if sudo systemctl is-active --quiet eavs 2>/dev/null; then
    sudo systemctl restart eavs
    log_success "eavs restarted"
  fi
}

# ==============================================================================
# Mode Selection
# ==============================================================================

select_user_mode() {
  log_step "User Mode Selection"

  echo
  echo "Oqto supports two user modes:"
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
  echo "Oqto can run agents in two modes:"
  echo
  echo -e "  ${BOLD}Local${NC} - Native processes"
  echo "    - Runs Pi, oqto-files, ttyd directly on host"
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
  # Use oqtoctl (same bcrypt implementation as the backend) for guaranteed compatibility
  # Try PATH first, then known install locations (PATH cache may be stale)
  local oqtoctl_bin=""
  if command_exists oqtoctl; then
    oqtoctl_bin="oqtoctl"
  elif [[ -x "${TOOLS_INSTALL_DIR}/oqtoctl" ]]; then
    oqtoctl_bin="${TOOLS_INSTALL_DIR}/oqtoctl"
  fi
  if [[ -n "$oqtoctl_bin" ]] && "$oqtoctl_bin" hash-password --help >/dev/null 2>&1; then
    echo -n "$password" | "$oqtoctl_bin" hash-password
  elif command_exists python3 && python3 -c "import bcrypt" 2>/dev/null; then
    # Fallback: python3 with bcrypt module
    python3 -c "import bcrypt, base64; pwd = base64.b64decode('$([[ -n "$password" ]] && echo -n "$password" | base64 -w0 || echo)').decode(); print(bcrypt.hashpw(pwd.encode(), bcrypt.gensalt(12)).decode())"
  else
    log_error "Cannot generate password hash. Install oqtoctl or python3 with bcrypt."
    exit 1
  fi
}

write_skdlr_agent_config() {
  local skdlr_config="/etc/oqto/skdlr-agent.toml"
  local sandbox_config="/etc/oqto/sandbox.toml"

  log_info "Writing skdlr agent config to $skdlr_config"

  sudo mkdir -p /etc/oqto

  # Ensure sandbox config exists for oqto-sandbox
  if [[ ! -f "$sandbox_config" ]]; then
    log_info "Creating default sandbox config at $sandbox_config"
    sudo cp "$SCRIPT_DIR/backend/crates/oqto/examples/sandbox.toml" "$sandbox_config"
    sudo chmod 644 "$sandbox_config"
  fi

  sudo tee "$skdlr_config" >/dev/null <<'EOF'
# skdlr config for Oqto sandboxed agents
# Forces all scheduled commands through oqto-sandbox

[executor]
wrapper = "oqto-sandbox"
wrapper_args = ["--config", "/etc/oqto/sandbox.toml", "--workspace", "{workdir}", "--"]
EOF

  sudo chmod 644 "$skdlr_config"
}

generate_config() {
  log_step "Generating configuration"

  # Create config directories
  mkdir -p "$OQTO_CONFIG_DIR"
  mkdir -p "$OQTO_DATA_DIR"

  local config_file="$OQTO_CONFIG_DIR/config.toml"

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
  log_info "Configuring Oqto..."

  # Workspace directory (use saved value as default if available)
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    WORKSPACE_DIR=$(prompt_input "Workspace directory" "${WORKSPACE_DIR:-$HOME/oqto/workspace}")
  else
    local default_workspace="/home/{linux_username}/oqto"
    WORKSPACE_DIR=$(prompt_input "Workspace base directory (user dirs created here)" "${WORKSPACE_DIR:-$default_workspace}")
  fi

  # Auth configuration (use globals so state persistence works)
  local dev_user_hash admin_user_hash=""

  if [[ "$OQTO_DEV_MODE" == "true" ]]; then
    log_info "Setting up development user..."
    dev_user_id=$(prompt_input "Dev user ID" "${dev_user_id:-dev}")
    dev_user_name=$(prompt_input "Dev user name" "${dev_user_name:-Developer}")
    dev_user_email=$(prompt_input "Dev user email" "${dev_user_email:-dev@localhost}")
    local dev_password
    dev_password=$(prompt_password "Dev user password")

    if [[ -n "$dev_password" ]]; then
      log_info "Generating password hash..."
      local oqtoctl_bin=""
      command_exists oqtoctl && oqtoctl_bin="oqtoctl"
      [[ -z "$oqtoctl_bin" && -x "${TOOLS_INSTALL_DIR}/oqtoctl" ]] && oqtoctl_bin="${TOOLS_INSTALL_DIR}/oqtoctl"
      if [[ -n "$oqtoctl_bin" ]]; then
        dev_user_hash=$("$oqtoctl_bin" hash-password --password "$dev_password")
      else
        dev_user_hash=$(generate_password_hash "$dev_password")
      fi
      dev_password=""
    else
      dev_user_hash=""
    fi
  elif [[ "$PRODUCTION_MODE" == "true" ]]; then
    # Production mode - admin hash is generated in create_admin_user_db step
    admin_user_hash=""
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
    echo "  - The 'oqto' group will be created"
    echo "  - Sudoers rules will allow managing octo_* users"
    echo
    if confirm "Enable Linux user isolation? (requires sudo)"; then
      linux_users_enabled="true"
      LINUX_USERS_ENABLED="true"
    fi
  fi

  # Determine Pi runtime mode based on backend mode and user mode
  # (needed early for runner_socket_pattern in [local] section)
  local pi_runtime_mode="local"
  if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
    pi_runtime_mode="container"
  elif [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
    pi_runtime_mode="runner"
  fi

  # Write config file
  log_info "Writing config to $config_file"

  cat >"$config_file" <<EOF
# Oqto Configuration
# Generated by setup.sh on $(date)

"\$schema" = "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/oqto/oqto.backend.config.schema.json"

profile = "default"

[logging]
level = "$OQTO_LOG_LEVEL"

[runtime]
timeout = 60
fail_fast = true

[backend]
mode = "$SELECTED_BACKEND_MODE"

[container]
runtime = "${CONTAINER_RUNTIME:-docker}"
default_image = "oqto-dev:latest"
base_port = 41820
EOF

  if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
    local runner_socket_line=""
    if [[ "$pi_runtime_mode" == "runner" ]]; then
      runner_socket_line='runner_socket_pattern = "/run/oqto/runner-sockets/{user}/oqto-runner.sock"'
    fi

    cat >>"$config_file" <<EOF

[local]
enabled = true
fileserver_binary = "oqto-files"
ttyd_binary = "ttyd"
workspace_dir = "$WORKSPACE_DIR"
single_user = $([[ "$SELECTED_USER_MODE" == "single" ]] && echo "true" || echo "false")
${runner_socket_line}

[local.linux_users]
enabled = $linux_users_enabled
prefix = "octo_"
uid_start = 2000
group = "oqto"
shell = "/bin/zsh"
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
dev_mode = $OQTO_DEV_MODE
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

  if [[ "$OQTO_DEV_MODE" == "true" && -n "${dev_user_hash:-}" ]]; then
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
public_dropbox = "${FEEDBACK_PUBLIC_DROPBOX:-/usr/local/share/oqto/issues}"
private_archive = "${FEEDBACK_PRIVATE_ARCHIVE:-/var/lib/oqto/issue-archive}"
keep_public = true
sync_interval_seconds = 60
EOF

  # runner_socket_pattern is added inline in the [local] block above

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

[agent_browser]
enabled = true
binary = "/usr/local/lib/oqto-browserd/bin/oqto-browserd.js"
headed = false
stream_port_base = 30000
stream_port_range = 10000
EOF

  log_success "Configuration written to $config_file"

  # API keys are now managed by EAVS, not stored in oqto's env file

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
    local creds_file="$OQTO_CONFIG_DIR/.admin_setup"
    # Write username/email normally, but use printf for the bcrypt hash
    # to avoid $2b$ being expanded as positional params when sourced
    {
      echo "ADMIN_USERNAME=\"$ADMIN_USERNAME\""
      echo "ADMIN_EMAIL=\"$ADMIN_EMAIL\""
      printf "ADMIN_PASSWORD_HASH='%s'\n" "$admin_user_hash"
    } >"$creds_file"
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

  local octo_group="oqto"
  local user_prefix="octo_"
  local server_user

  # Determine who will run the backend:
  # - For system service (multi-user production): use 'oqto' system user
  # - For user service (development): use current user
  if [[ "${MULTI_USER:-false}" == "true" ]] && [[ "$OS" == "linux" ]]; then
    # Production multi-user mode: backend runs as 'oqto' system user
    server_user="oqto"
    ensure_octo_system_user
  else
    # Development mode: backend runs as current user
    server_user=$(whoami)
  fi

  log_info "Sudoers rules will be configured for user: $server_user"

  # 1. Create the oqto group
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

  # 2. Add the server user to the oqto group
  log_info "Adding user '$server_user' to group '$octo_group'..."
  if ! sudo usermod -aG "$octo_group" "$server_user"; then
    log_warn "Failed to add user to group (may need to re-login)"
  else
    log_success "User '$server_user' added to group '$octo_group'"
  fi

  # 3. Create sudoers file for multi-user process management
  log_info "Configuring sudoers for multi-user process management..."

  local sudoers_file="/etc/sudoers.d/oqto-multiuser"
  # Note: uid_start comes from config, default 2000
  local uid_start="${OQTO_UID_START:-2000}"
  # UID regex: match 2000-60000 range (avoids system UIDs below and nobody/reserved above)
  # [2-9][0-9]{3} matches 2000-9999, [1-5][0-9]{4} matches 10000-59999, 60000 exact
  local uid_regex="([2-9][0-9][0-9][0-9]|[1-5][0-9][0-9][0-9][0-9]|60000)"

  local sudoers_content
  sudoers_content=$(
    cat <<SUDOERS_EOF
# Oqto Multi-User Process Isolation - SECURE VERSION
# Generated by setup.sh on $(date)
# Allows the oqto server user to manage isolated user accounts
#
# SECURITY: Uses regex patterns (^...\$) to prevent privilege escalation.
# - UIDs restricted to ${uid_start}-60000 range (avoids system UIDs and nobody/reserved)
# - Usernames must start with ${user_prefix} prefix
# - Workspace chown restricted to ${user_prefix}* home directories only
# Requires sudo 1.9.10+ for regex support.

# Group management - only create the ${octo_group} group (safe - fixed value)
Cmnd_Alias OQTO_GROUPADD = /usr/sbin/groupadd ${octo_group}

# User creation - RESTRICTED to safe UID range and ${user_prefix} prefix
# Regex matches: -u NNNN -g ${octo_group} -s /bin/bash -m/-M -c COMMENT USERNAME
# UID must be in ${uid_start}-60000 range, username must start with ${user_prefix}
# GECOS format: "Oqto platform user: USER_ID" - use .* to match including spaces
Cmnd_Alias OQTO_USERADD = \\
    /usr/sbin/useradd ^-u ${uid_regex} -g ${octo_group} -s /bin/bash -m -c .* ${user_prefix}[a-z0-9_-]+\$, \\
    /usr/sbin/useradd ^-u ${uid_regex} -g ${octo_group} -s /bin/bash -M -c .* ${user_prefix}[a-z0-9_-]+\$

# User deletion - only ${user_prefix} users, no home removal (-r flag not allowed)
Cmnd_Alias OQTO_USERDEL = /usr/sbin/userdel ^${user_prefix}[a-z0-9_-]+\$

# Directory creation for runner sockets - RESTRICTED path (no path traversal)
Cmnd_Alias OQTO_MKDIR = /bin/mkdir ^-p /run/oqto/runner-sockets/${user_prefix}[a-z0-9_-]+\$

# Runner socket ownership - RESTRICTED to exact paths
Cmnd_Alias OQTO_CHOWN_RUNNER = \\
    /usr/bin/chown ^${user_prefix}[a-z0-9_-]+\\:${octo_group} /run/oqto/runner-sockets/${user_prefix}[a-z0-9_-]+\$

# Workspace ownership - RESTRICTED to ${user_prefix} user home directories ONLY
# SECURITY: Only allows chown on /home/${user_prefix}*/... NOT on other users' homes
# The regex ensures the path starts with /home/${user_prefix} to prevent privilege escalation
Cmnd_Alias OQTO_CHOWN_WORKSPACE = \\
    /usr/bin/chown ^-R ${user_prefix}[a-z0-9_-]+\\:${octo_group} /home/${user_prefix}[a-z0-9_-]+(/[^.][^/]*)*\$

# Permissions for runner socket directories
Cmnd_Alias OQTO_CHMOD_RUNNER = /usr/bin/chmod ^2770 /run/oqto/runner-sockets/${user_prefix}[a-z0-9_-]+\$

# systemd linger - only for ${user_prefix} users
Cmnd_Alias OQTO_LINGER = /usr/bin/loginctl ^enable-linger ${user_prefix}[a-z0-9_]+\$

# Start user systemd instance - RESTRICTED to ${user_prefix} user UIDs
Cmnd_Alias OQTO_START_USER = /usr/bin/systemctl ^start user@${uid_regex}\\.service\$

# User management - group and user creation
${server_user} ALL=(root) NOPASSWD: OQTO_GROUPADD, OQTO_USERADD

# systemd user management - enable/start oqto-runner as ${user_prefix}* users
Cmnd_Alias OQTO_RUNNER_SYSTEMCTL = \\
    /usr/bin/systemctl --user enable --now oqto-runner, \\
    /usr/bin/systemctl --user start oqto-runner, \\
    /usr/bin/systemctl --user enable oqto-runner
${server_user} ALL=(${user_prefix}*) NOPASSWD: OQTO_RUNNER_SYSTEMCTL

# Runner socket directory setup and workspace ownership
${server_user} ALL=(root) NOPASSWD: OQTO_MKDIR, OQTO_CHOWN_RUNNER, OQTO_CHOWN_WORKSPACE, OQTO_CHMOD_RUNNER

# User systemd management
${server_user} ALL=(root) NOPASSWD: OQTO_START_USER, OQTO_LINGER
SUDOERS_EOF
  )

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
  local workspace_base="/var/lib/oqto/workspaces"
  sudo mkdir -p "$workspace_base"
  sudo chown root:"$octo_group" "$workspace_base"
  sudo chmod 775 "$workspace_base"
  log_success "Workspace directory created: $workspace_base"

  # 5. Install system sandbox config (trusted, root-owned)
  log_info "Installing system sandbox configuration..."
  local sandbox_config="/etc/oqto/sandbox.toml"
  sudo mkdir -p /etc/oqto

  sudo tee "$sandbox_config" >/dev/null <<'EOF'
# Oqto Sandbox Configuration (System-wide)
# This file is owned by root and trusted by oqto-runner.
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
    "/etc/oqto/sandbox.toml",
]

# Namespace isolation
isolate_network = false
isolate_pid = true
EOF

  sudo chmod 644 "$sandbox_config"
  sudo chown root:root "$sandbox_config"
  log_success "System sandbox config installed: $sandbox_config"

  # 6. Install skdlr agent config (forces oqto-sandbox wrapper)
  log_info "Installing skdlr agent configuration..."
  local skdlr_config="/etc/oqto/skdlr-agent.toml"

  sudo tee "$skdlr_config" >/dev/null <<'EOF'
# Oqto skdlr configuration for agent scheduling
# This file is owned by root and enforces oqto-sandbox for scheduled runs.

[executor]
wrapper = "oqto-sandbox"
wrapper_args = [
    "--config", "/etc/oqto/sandbox.toml",
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

  log_step "Creating admin user"

  local admin_user="${ADMIN_USERNAME:-}"
  local admin_email="${ADMIN_EMAIL:-}"

  # Load from creds file if available
  local creds_file="$OQTO_CONFIG_DIR/.admin_setup"
  if [[ -f "$creds_file" ]]; then
    # shellcheck source=/dev/null
    source "$creds_file"
    admin_user="${ADMIN_USERNAME:-$admin_user}"
    admin_email="${ADMIN_EMAIL:-$admin_email}"
  fi

  if [[ -z "$admin_user" || -z "$admin_email" ]]; then
    log_warn "Admin username/email not set. Re-run setup or create manually:"
    log_info "  oqtoctl user bootstrap --username <user> --email <email>"
    return 0
  fi

  # Find oqtoctl
  local oqtoctl_bin=""
  if [[ -x "${TOOLS_INSTALL_DIR}/oqtoctl" ]]; then
    oqtoctl_bin="${TOOLS_INSTALL_DIR}/oqtoctl"
  elif command_exists oqtoctl; then
    oqtoctl_bin="oqtoctl"
  else
    log_error "oqtoctl not found. Run the build step first."
    return 1
  fi

  # Ensure database exists by starting the service (runs migrations)
  local db_path=""
  if [[ "${SELECTED_USER_MODE:-}" == "multi" ]]; then
    db_path="/var/lib/oqto/.local/share/oqto/oqto.db"
    # Migrate from old name
    local old_db="/var/lib/oqto/.local/share/oqto/sessions.db"
    if [[ -f "$old_db" && ! -f "$db_path" ]]; then
      sudo mv "$old_db" "$db_path"
      sudo mv "${old_db}-wal" "${db_path}-wal" 2>/dev/null || true
      sudo mv "${old_db}-shm" "${db_path}-shm" 2>/dev/null || true
    fi
    sudo mkdir -p "$(dirname "$db_path")"
    sudo chown -R oqto:oqto /var/lib/oqto/.local
  else
    local data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
    db_path="${data_dir}/oqto/oqto.db"
    mkdir -p "$(dirname "$db_path")"
  fi

  if [[ ! -f "$db_path" ]]; then
    log_info "Starting service to initialize database..."
    if [[ "${SELECTED_USER_MODE:-}" == "multi" ]]; then
      sudo systemctl start oqto 2>/dev/null || true
    else
      systemctl --user start oqto 2>/dev/null || true
    fi
    # Wait for DB to appear
    local retries=0
    while [[ ! -f "$db_path" && $retries -lt 15 ]]; do
      sleep 1
      retries=$((retries + 1))
    done
    # Stop the service again so bootstrap can write to the DB
    if [[ "${SELECTED_USER_MODE:-}" == "multi" ]]; then
      sudo systemctl stop oqto 2>/dev/null || true
    else
      systemctl --user stop oqto 2>/dev/null || true
    fi
  fi

  if [[ ! -f "$db_path" ]]; then
    log_warn "Database not found at $db_path"
    log_info "Create admin user manually after starting Oqto:"
    log_info "  $oqtoctl_bin user bootstrap --username \"$admin_user\" --email \"$admin_email\""
    return 0
  fi

  # Check if user already exists
  if command_exists sqlite3; then
    local existing
    if [[ "${SELECTED_USER_MODE:-}" == "multi" ]]; then
      existing=$(sudo sqlite3 "$db_path" "SELECT COUNT(*) FROM users WHERE username = '$admin_user';" 2>/dev/null || echo "0")
    else
      existing=$(sqlite3 "$db_path" "SELECT COUNT(*) FROM users WHERE username = '$admin_user';" 2>/dev/null || echo "0")
    fi
    if [[ "$existing" -gt 0 ]]; then
      log_info "Admin user '$admin_user' already exists, skipping"
      rm -f "$creds_file"
      return 0
    fi
  fi

  # Hash the password first (runs as current user -- no DB access needed)
  local admin_hash=""
  if [[ "$NONINTERACTIVE" == "true" ]]; then
    local admin_password
    admin_password=$(generate_secure_secret 16)
    admin_hash=$("$oqtoctl_bin" hash-password --password "$admin_password")
    log_info "Generated admin password: $admin_password"
    log_warn "SAVE THIS PASSWORD - it will not be shown again!"
  else
    log_info "Set the admin password:"
    admin_hash=$("$oqtoctl_bin" hash-password)
  fi

  if [[ -z "$admin_hash" ]]; then
    log_error "Failed to hash password"
    return 1
  fi

  # Build bootstrap args with pre-computed hash (no interactive prompts needed)
  local bootstrap_args=(user bootstrap --username "$admin_user" --email "$admin_email")
  bootstrap_args+=(--database "$db_path")
  bootstrap_args+=(--password-hash "$admin_hash")
  # Skip Linux user creation during setup -- it happens at first login via oqto-usermgr
  bootstrap_args+=(--no-linux-user)

  # In multi-user mode, run as oqto user (DB is owned by oqto)
  local run_prefix=()
  if [[ "${SELECTED_USER_MODE:-}" == "multi" ]]; then
    run_prefix=(sudo -u oqto)
  fi

  if "${run_prefix[@]}" "$oqtoctl_bin" "${bootstrap_args[@]}"; then
    log_success "Admin user '$admin_user' created"
  else
    log_warn "Failed to create admin user. Create manually:"
    log_info "  sudo -u oqto $oqtoctl_bin user bootstrap --username \"$admin_user\" --email \"$admin_email\""
    return 0
  fi

  # Generate an initial invite code
  generate_initial_invite_code

  # Clean up
  rm -f "$creds_file"
}

generate_initial_invite_code() {
  log_step "Generating initial invite code"

  echo
  echo "To add additional users, you'll need invite codes."
  echo "An initial invite code will be generated when you start Oqto."
  echo
  echo "After starting the server, create invite codes with:"
  echo "  oqto invites create --uses 1"
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
  if [[ "$OQTO_HARDEN_SERVER" == "yes" ]]; then
    return 0
  elif [[ "$OQTO_HARDEN_SERVER" == "no" ]]; then
    return 1
  fi

  # Prompt user
  if [[ "$NONINTERACTIVE" != "true" ]]; then
    if confirm "Apply server hardening (firewall, fail2ban, SSH hardening)?"; then
      OQTO_HARDEN_SERVER="yes"
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
  if [[ "$OQTO_SETUP_FIREWALL" != "yes" ]]; then
    log_info "Skipping firewall configuration"
    return
  fi

  log_step "Configuring firewall"

  local ssh_port="${OQTO_SSH_PORT:-22}"
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

      # Allow Oqto ports (only if not using Caddy)
      if [[ "$SETUP_CADDY" != "yes" ]]; then
        sudo ufw allow "$octo_port/tcp" comment 'Oqto API'
        sudo ufw allow "$frontend_port/tcp" comment 'Oqto Frontend'
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

      # Allow Oqto ports (only if not using Caddy)
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
  if [[ "$OQTO_SETUP_FAIL2BAN" != "yes" ]]; then
    log_info "Skipping fail2ban configuration"
    return
  fi

  log_step "Configuring fail2ban"

  local ssh_port="${OQTO_SSH_PORT:-22}"
  local jail_local="/etc/fail2ban/jail.local"

  # Create jail.local configuration
  sudo tee "$jail_local" >/dev/null <<EOF
# Fail2ban Configuration for Oqto Server
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

  # Wait for service to be ready, then show status
  sleep 2
  sudo fail2ban-client status 2>/dev/null || log_warn "fail2ban started but client not ready yet (check: sudo fail2ban-client status)"
  log_success "fail2ban configured"
}

# Harden SSH configuration
harden_ssh() {
  if [[ "$OQTO_HARDEN_SSH" != "yes" ]]; then
    log_info "Skipping SSH hardening"
    return
  fi

  log_step "Hardening SSH configuration"

  local ssh_port="${OQTO_SSH_PORT:-22}"
  local sshd_config_dir="/etc/ssh/sshd_config.d"
  local hardening_conf="$sshd_config_dir/00-oqto-hardening.conf"

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
# SSH Hardening Configuration for Oqto Server
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

  # Add AllowUsers - always include the current server user (who runs setup)
  # plus the admin username if different
  local ssh_allowed_users="$(whoami)"
  if [[ -n "$ADMIN_USERNAME" && "$ADMIN_USERNAME" != "$(whoami)" ]]; then
    ssh_allowed_users="$ssh_allowed_users $ADMIN_USERNAME"
  fi
  # Also include root in case of emergency console access
  if [[ "$ssh_allowed_users" != *"root"* ]]; then
    ssh_allowed_users="$ssh_allowed_users root"
  fi
  echo "AllowUsers $ssh_allowed_users" | sudo tee -a "$hardening_conf" >/dev/null
  log_info "SSH AllowUsers: $ssh_allowed_users"

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
  if [[ "$OQTO_SETUP_AUTO_UPDATES" != "yes" ]]; then
    log_info "Skipping automatic updates configuration"
    return
  fi

  log_step "Configuring automatic security updates"

  case "$OS_DISTRO" in
  debian | ubuntu | pop | linuxmint)
    # Configure unattended-upgrades
    sudo tee /etc/apt/apt.conf.d/50unattended-upgrades >/dev/null <<'EOF'
// Automatic security updates configuration
// Generated by Oqto setup.sh

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
system_name = oqto-server
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
  if [[ "$OQTO_HARDEN_KERNEL" != "yes" ]]; then
    log_info "Skipping kernel hardening"
    return
  fi

  log_step "Applying kernel security parameters"

  local sysctl_conf="/etc/sysctl.d/99-oqto-hardening.conf"

  sudo tee "$sysctl_conf" >/dev/null <<'EOF'
# Kernel Security Parameters for Oqto Server
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
  log_warn "║  SSH port will be set to: ${OQTO_SSH_PORT:-22}                   ║"
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
  echo "  - SSH hardening:   ${OQTO_HARDEN_SSH}"
  echo "  - Auto updates:    ${OQTO_SETUP_AUTO_UPDATES}"
  echo "  - Kernel hardening: ${OQTO_HARDEN_KERNEL}"
  echo "  - Audit logging:   $(systemctl is-active auditd 2>/dev/null || echo 'not running')"
  echo
}

# ==============================================================================
# Service Installation
# ==============================================================================

# Ensure the oqto system user exists with a proper home directory.
# Called early in multi-user setup so EAVS/hstry/mmry config can be
# written into ~oqto/.config/ before services are installed.
# Safe to call multiple times (idempotent).
OQTO_HOME="/home/oqto"

ensure_octo_system_user() {
  if id oqto &>/dev/null; then
    OQTO_HOME=$(eval echo "~oqto")
    return 0
  fi

  log_info "Creating oqto system user with home at $OQTO_HOME..."
  # Use /bin/bash so admins can: sudo -su oqto
  # No password set, so direct/SSH login is impossible.
  sudo useradd -r -m -d "$OQTO_HOME" -s /bin/bash oqto

  # Create XDG directory structure
  sudo mkdir -p \
    "${OQTO_HOME}/.config" \
    "${OQTO_HOME}/.local/share" \
    "${OQTO_HOME}/.local/state"
  sudo chown -R oqto:oqto "$OQTO_HOME"

  log_success "Created oqto system user (home: $OQTO_HOME)"
}

install_service_linux() {
  log_step "Installing systemd service"

  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    # User-level service
    local service_dir="$HOME/.config/systemd/user"
    mkdir -p "$service_dir"

    local service_file="$service_dir/oqto.service"

    cat >"$service_file" <<EOF
# Oqto Server - User service
# Generated by setup.sh

[Unit]
Description=Oqto Server (User Mode)
After=default.target

[Service]
Type=simple
Environment=OQTO_CONFIG=$OQTO_CONFIG_DIR/config.toml
Environment=RUST_LOG=$OQTO_LOG_LEVEL
Environment=PLAYWRIGHT_BROWSERS_PATH=/usr/local/share/playwright-browsers
EnvironmentFile=-$OQTO_CONFIG_DIR/env
ExecStart=/usr/local/bin/oqto serve --local-mode
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
      systemctl --user enable oqto
      systemctl --user start oqto
      log_success "Service enabled and started"
      log_info "Check status with: systemctl --user status oqto"
      log_info "View logs with: journalctl --user -u oqto -f"
    else
      log_info "To enable manually:"
      log_info "  systemctl --user daemon-reload"
      log_info "  systemctl --user enable --now oqto"
    fi
  else
    # System-level service (requires sudo)
    log_info "Multi-user mode requires system-level service installation"

    if ! confirm "Install system service? (requires sudo)"; then
      log_info "Skipping service installation"
      return
    fi

    # Ensure oqto user exists (may already be created by ensure_octo_system_user)
    ensure_octo_system_user
    local octo_home="$OQTO_HOME"

    # Create runtime directories
    sudo mkdir -p /run/oqto
    sudo chown oqto:oqto /run/oqto

    # Runtime config in oqto's home (XDG layout: ~/.config/oqto/)
    # This is what the oqto service actually reads at startup.
    local octo_config_home="${octo_home}/.config/oqto"
    sudo mkdir -p "$octo_config_home"
    sudo cp "$OQTO_CONFIG_DIR/config.toml" "${octo_config_home}/config.toml"
    if [[ -f "$OQTO_CONFIG_DIR/env" ]]; then
      sudo cp "$OQTO_CONFIG_DIR/env" "${octo_config_home}/env"
      sudo chmod 600 "${octo_config_home}/env"
    fi

    # Also copy a baseline config to /etc/oqto/ for sandbox policy reference.
    # Sandbox configs (sandbox.toml, skdlr-agent.toml) live here too - these
    # are system-wide policy the admin controls, not per-service runtime config.
    sudo mkdir -p /etc/oqto
    sudo cp "$OQTO_CONFIG_DIR/config.toml" /etc/oqto/config.toml

    sudo chown -R oqto:oqto "$octo_home"

    # Install service file
    local service_file="/etc/systemd/system/oqto.service"

    sudo tee "$service_file" >/dev/null <<EOF
# Oqto Server - System service
# Generated by setup.sh

[Unit]
Description=Oqto Control Plane Server
After=network.target eavs.service oqto-usermgr.service
Wants=eavs.service oqto-usermgr.service

[Service]
Type=simple
User=oqto
Group=oqto
WorkingDirectory=${octo_home}
Environment=HOME=${octo_home}
Environment=XDG_CONFIG_HOME=${octo_home}/.config
Environment=XDG_DATA_HOME=${octo_home}/.local/share
Environment=XDG_STATE_HOME=${octo_home}/.local/state
Environment=OQTO_CONFIG=${octo_config_home}/config.toml
Environment=RUST_LOG=$OQTO_LOG_LEVEL
Environment=PLAYWRIGHT_BROWSERS_PATH=/usr/local/share/playwright-browsers
EnvironmentFile=-${octo_config_home}/env
ExecStart=/usr/local/bin/oqto serve
ExecStop=/bin/kill -TERM \$MAINPID
TimeoutStopSec=30
Restart=on-failure
RestartSec=5
ProtectSystem=strict
ReadWritePaths=${octo_home}
ReadWritePaths=/run/oqto
PrivateTmp=true
NoNewPrivileges=true
AmbientCapabilities=CAP_NET_BIND_SERVICE
EOF

    sudo tee -a "$service_file" >/dev/null <<EOF

[Install]
WantedBy=multi-user.target
EOF

    # Binaries are already in /usr/local/bin from build_octo

    log_success "Service file created: $service_file"

    # Install oqto-usermgr service (privileged user management daemon)
    install_usermgr_service

    # Ensure runner socket base directory exists at boot
    install_runner_socket_dirs

    if confirm "Enable and start the service now?"; then
      sudo systemctl daemon-reload
      sudo systemctl enable oqto
      sudo systemctl start oqto
      log_success "Service enabled and started"
      log_info "Check status with: sudo systemctl status oqto"
      log_info "View logs with: sudo journalctl -u oqto -f"
    fi
  fi
}

install_usermgr_service() {
  # oqto-usermgr runs as root and listens on a unix socket.
  # The oqto service (unprivileged) sends JSON requests over the socket
  # to create/delete Linux users, manage directories, etc.
  # This provides OS-level privilege separation.
  log_info "Installing oqto-usermgr service..."

  local service_file="/etc/systemd/system/oqto-usermgr.service"
  sudo tee "$service_file" >/dev/null <<EOF
[Unit]
Description=Oqto User Manager (privileged helper)
Before=oqto.service

[Service]
Type=simple
ExecStart=/usr/local/bin/oqto-usermgr
Restart=on-failure
RestartSec=3
RuntimeDirectory=oqto
RuntimeDirectoryMode=0755
RuntimeDirectoryPreserve=yes
ProtectSystem=strict
ReadWritePaths=/etc
ReadWritePaths=/home
ReadWritePaths=/run/oqto
ReadWritePaths=/var/lib/oqto
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

  sudo systemctl daemon-reload
  sudo systemctl enable oqto-usermgr
  sudo systemctl start oqto-usermgr
  log_success "oqto-usermgr service installed and started"
}

install_runner_socket_dirs() {
  # Ensure the shared runner socket base directory exists at boot.
  # Per-user subdirectories are created by oqto-usermgr at user creation time.
  # Per-user service files (oqto-runner, hstry, mmry) are also created by usermgr.
  log_info "Setting up runner socket directories..."

  local tmpfiles_conf="/etc/tmpfiles.d/oqto-runner.conf"

  sudo tee "$tmpfiles_conf" >/dev/null <<'EOF'
d /run/oqto/runner-sockets 2770 root oqto -
EOF

  sudo systemd-tmpfiles --create "$tmpfiles_conf" >/dev/null 2>&1 || true

  # Remove any stale global service template (usermgr now handles per-user service files)
  if [[ -f /etc/systemd/user/oqto-runner.service ]]; then
    sudo rm -f /etc/systemd/user/oqto-runner.service
    log_info "Removed stale global oqto-runner.service template"
  fi

  log_success "Runner socket directories configured"
}

install_service_macos() {
  log_step "Installing launchd service"

  local plist_dir="$HOME/Library/LaunchAgents"
  local log_dir="$HOME/Library/Logs"
  mkdir -p "$plist_dir" "$log_dir"

  local plist_file="$plist_dir/ai.oqto.server.plist"

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
    <string>ai.oqto.server</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/oqto</string>
        <string>serve</string>
        $serve_flags
    </array>

    <key>EnvironmentVariables</key>
    <dict>
        <key>OQTO_CONFIG</key>
        <string>$OQTO_CONFIG_DIR/config.toml</string>
        <key>RUST_LOG</key>
        <string>$OQTO_LOG_LEVEL</string>
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
    <string>$log_dir/oqto.stdout.log</string>

    <key>StandardErrorPath</key>
    <string>$log_dir/oqto.stderr.log</string>
</dict>
</plist>
EOF

  log_success "Launchd plist created: $plist_file"

  if confirm "Load and start the service now?"; then
    # Unload if already loaded
    launchctl unload "$plist_file" 2>/dev/null || true
    launchctl load "$plist_file"
    log_success "Service loaded and started"
    log_info "Check status with: launchctl list | grep oqto"
    log_info "View logs at: $log_dir/oqto.*.log"
  else
    log_info "To load manually:"
    log_info "  launchctl load $plist_file"
  fi
}

install_service() {
  if [[ "$OQTO_INSTALL_SERVICE" != "yes" ]]; then
    log_info "Skipping service installation (OQTO_INSTALL_SERVICE=no)"
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

  if ! confirm "Build the Oqto container image? (this may take several minutes)"; then
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
  $CONTAINER_RUNTIME build -t oqto-dev:latest -f "$dockerfile" .

  log_success "Container image built: oqto-dev:latest"
}

# ==============================================================================
# Summary and Next Steps
# ==============================================================================

start_all_services() {
  log_step "Starting services"

  local is_user_service="false"
  if [[ "$SELECTED_USER_MODE" == "single" ]]; then
    is_user_service="true"
  fi

  start_svc() {
    local svc="$1"
    local user_svc="${2:-false}"

    if [[ "$user_svc" == "true" ]]; then
      if systemctl --user is-active "$svc" &>/dev/null; then
        log_success "$svc: already running"
      elif systemctl --user is-enabled "$svc" &>/dev/null; then
        systemctl --user start "$svc" && log_success "$svc: started" || log_warn "$svc: failed to start"
      fi
    else
      if sudo systemctl is-active "$svc" &>/dev/null; then
        log_success "$svc: already running"
      elif sudo systemctl is-enabled "$svc" &>/dev/null; then
        sudo systemctl start "$svc" && log_success "$svc: started" || log_warn "$svc: failed to start"
      fi
    fi
  }

  # Core services (restart to pick up rebuilt binaries)
  if [[ "$is_user_service" == "false" ]]; then
    # Multi-user: restart system services to pick up new binaries
    for svc in oqto-usermgr eavs oqto; do
      if sudo systemctl is-active "$svc" &>/dev/null; then
        sudo systemctl restart "$svc" && log_success "$svc: restarted" || log_warn "$svc: failed to restart"
      elif sudo systemctl is-enabled "$svc" &>/dev/null; then
        sudo systemctl start "$svc" && log_success "$svc: started" || log_warn "$svc: failed to start"
      fi
    done

    # Re-provision all existing platform users' runner services.
    # The usermgr was just restarted with new service file templates,
    # so we need to push updated service files to all octo_* users and
    # restart their runners.
    log_info "Updating per-user services for existing platform users..."
    for user_home in /home/octo_*; do
      local username
      username=$(basename "$user_home")
      local uid
      uid=$(id -u "$username" 2>/dev/null) || continue

      log_info "Updating services for $username (uid=$uid)..."

      # Stop existing user services (they have stale service files)
      local runtime_dir="/run/user/$uid"
      local bus="unix:path=${runtime_dir}/bus"
      sudo runuser -u "$username" -- env \
        XDG_RUNTIME_DIR="$runtime_dir" \
        DBUS_SESSION_BUS_ADDRESS="$bus" \
        systemctl --user stop oqto-runner hstry mmry 2>/dev/null || true

      # Remove stale socket
      sudo rm -f "/run/oqto/runner-sockets/${username}/oqto-runner.sock"

      # Trigger usermgr to rewrite service files and restart.
      # The usermgr socket is owned by oqto:root 0600, so we must run as oqto.
      if [[ -S /run/oqto/usermgr.sock ]]; then
        local response
        response=$(sudo -u oqto python3 -c "
import socket, json, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect('/run/oqto/usermgr.sock')
req = json.dumps({'cmd': 'setup-user-runner', 'args': {'username': '${username}', 'uid': ${uid}}}) + '\n'
s.sendall(req.encode())
data = b''
while True:
    chunk = s.recv(4096)
    if not chunk: break
    data += chunk
    if b'\n' in data: break
s.close()
print(data.decode().strip())
" 2>/dev/null)
        if echo "$response" | grep -q '"ok":true'; then
          log_success "$username: services updated"
        else
          log_warn "$username: setup-user-runner failed: $response"
        fi
      fi
    done
  else
    start_svc eavs "$is_user_service"
    start_svc oqto "$is_user_service"
  fi

  # Reverse proxy
  if [[ "$SETUP_CADDY" == "yes" ]]; then
    start_svc caddy
  fi

  # Optional services
  if sudo systemctl is-enabled searxng &>/dev/null || systemctl --user is-enabled searxng &>/dev/null; then
    start_svc searxng "$is_user_service"
  fi

  # Restart oqto to pick up any config changes from this setup run
  if [[ "$is_user_service" == "true" ]]; then
    systemctl --user restart oqto &>/dev/null || true
  else
    sudo systemctl restart oqto &>/dev/null || true
  fi
  log_success "All services started"
}

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
  echo -e "  Oqto backend:   $(check_service_status oqto "$is_user_service")"

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
    if [[ "$SELECTED_USER_MODE" == "multi" ]]; then
      echo -e "  hstry:          ${CYAN}per-user (managed by runner)${NC}"
    else
      echo -e "  hstry:          $(check_service_status hstry "$is_user_service")"
    fi
  fi

  echo
  echo "============================================================"
  echo "                    CONFIGURATION"
  echo "============================================================"
  echo
  echo "  User mode:       $SELECTED_USER_MODE"
  echo "  Backend mode:    $SELECTED_BACKEND_MODE"
  echo "  Deployment mode: $([[ "$PRODUCTION_MODE" == "true" ]] && echo "Production" || echo "Development")"
  echo "  Config file:     $OQTO_CONFIG_DIR/config.toml"
  echo

  if [[ "$PRODUCTION_MODE" == "true" ]]; then
    echo "  Security:"
    echo "    JWT secret:    configured (64 characters)"
    echo "    Admin user:    $ADMIN_USERNAME"
    echo "    Admin email:   $ADMIN_EMAIL"
    if [[ "$NONINTERACTIVE" == "true" ]]; then
      echo -e "    ${YELLOW}Admin password was shown during setup${NC}"
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

    if [[ "$OQTO_HARDEN_SERVER" == "yes" && "$OS" == "linux" ]]; then
      echo "  Server Hardening:"
      echo "    Firewall:      $(command_exists ufw && echo 'UFW enabled' || (command_exists firewall-cmd && echo 'firewalld enabled' || echo 'not configured'))"
      echo -e "    Fail2ban:      $(check_service_status fail2ban)"
      echo "    SSH port:      ${OQTO_SSH_PORT:-22}"
      echo "    SSH auth:      public key only (password disabled)"
      echo "    Auto updates:  ${OQTO_SETUP_AUTO_UPDATES}"
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
  echo -e "    oqto:          $(check_bin oqto)"
  echo -e "    eavs:          $(check_bin eavs)"
  echo -e "    oqto-files:    $(check_bin oqto-files)"
  echo -e "    pi:            $(check_bin pi)"
  if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
    echo -e "    ttyd:          $(check_bin ttyd)"
  fi
  if [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
    echo -e "    oqto-runner:   $(check_bin oqto-runner)"
  fi
  if [[ "$SELECTED_BACKEND_MODE" == "container" ]]; then
    echo -e "    pi-bridge:     $(check_bin pi-bridge)"
  fi
  echo

  echo "  Agent tools:"
  for tool in agntz mmry scrpr sx tmpltr sldr ignr typst slidev; do
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
    service_needs_start oqto && need_start+=("oqto")
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
    echo "     oqtoctl invites create --uses 1"
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

    if [[ "$OQTO_DEV_MODE" == "true" && -n "${dev_user_id:-}" ]]; then
      echo "  $step. Login with your dev credentials:"
      echo "     Username: $dev_user_id"
      echo "     Password: (the password you entered)"
      echo
      ((step++))
    fi
  fi

  # Show API key warning if not configured
  if [[ "$EAVS_ENABLED" != "true" && "$LLM_API_KEY_SET" != "true" && -n "$LLM_PROVIDER" ]]; then
    echo -e "  ${YELLOW}IMPORTANT:${NC} Set your API key before starting Oqto:"
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
    echo "     source $OQTO_CONFIG_DIR/env"
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
________  ________  _________  ________
|\   __  \|\   __  \|\___   ___\\   __  \
\ \  \|\  \ \  \|\  \|___ \  \_\ \  \|\  \
 \ \  \\\  \ \  \\\  \   \ \  \ \ \  \\\  \
  \ \  \\\  \ \  \\\  \   \ \  \ \ \  \\\  \
   \ \_______\ \_____  \   \ \__\ \ \_______\
    \|_______|\|___| \__\   \|__|  \|_______|
                    \|__|

            got tentacles?

Oqto Setup Script

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
                        Example: --domain oqto.example.com
  
  --ssh-port <port>     Set SSH port for hardening (default: 22)
  
  Disable specific hardening features (use with --production):
  --no-firewall         Skip firewall configuration
  --no-fail2ban         Skip fail2ban installation
  --no-ssh-hardening    Skip SSH hardening (keeps password auth)
  --no-auto-updates     Skip automatic security updates
  --no-kernel-hardening Skip kernel sysctl hardening
  
  State management:
  --update              Pull latest code, rebuild, deploy, and restart services
  --fresh               Clear all saved state and completed steps, start over
  --redo step1,step2    Re-run specific steps (comma-separated)
                        (state: ~/.config/oqto/setup-state.env)
                        (steps: ~/.config/oqto/setup-steps-done)

  Tool installation:
  --all-tools           Install all byteowlz agent tools
  --no-agent-tools      Skip agent tools installation

Environment Variables:
  OQTO_USER_MODE          single or multi (default: single)
  OQTO_BACKEND_MODE       local or container (default: local)
  OQTO_CONTAINER_RUNTIME  docker, podman, or auto (default: auto)
  OQTO_INSTALL_DEPS       yes or no (default: yes)
  OQTO_INSTALL_SERVICE    yes or no (default: yes)
  OQTO_INSTALL_AGENT_TOOLS yes or no (default: yes)
  OQTO_DEV_MODE           true or false (default: prompt user)
  OQTO_LOG_LEVEL          error, warn, info, debug, trace (default: info)
  OQTO_SETUP_CADDY        yes or no (default: prompt user in production mode)
  OQTO_DOMAIN             domain for HTTPS (e.g., oqto.example.com)

Server Hardening (Linux production mode only):
  OQTO_HARDEN_SERVER      yes or no (default: prompt in production mode)
  OQTO_SSH_PORT           SSH port number (default: 22)
  OQTO_SETUP_FIREWALL     yes or no - configure UFW/firewalld (default: yes)
  OQTO_SETUP_FAIL2BAN     yes or no - install and configure fail2ban (default: yes)
  OQTO_HARDEN_SSH         yes or no - apply SSH hardening (default: yes)
  OQTO_SETUP_AUTO_UPDATES yes or no - enable automatic security updates (default: yes)
  OQTO_HARDEN_KERNEL      yes or no - apply kernel security parameters (default: yes)

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
  ignr    - Gitignore generation (auto-detect)

Other Tools:
  ttyd    - Web terminal
  pi      - Main chat interface (primary agent harness)

Search Engine:
  SearXNG - Local privacy-respecting metasearch engine (for sx)
  Valkey  - In-memory cache for SearXNG rate limiting

Pi Extensions (from github.com/byteowlz/pi-agent-extensions):
  auto-rename          - Auto-generate session names from first query
  oqto-bridge          - Emit agent phase status for the Oqto runner
  oqto-todos           - Todo management for Oqto UI
  custom-context-files - Auto-load USER.md, PERSONALITY.md into prompts

For detailed documentation on all prerequisites and components, see SETUP.md

Examples:
  # Interactive setup (recommended for first-time)
  ./setup.sh

  # Quick development setup (no prompts)
  ./setup.sh --dev

  # Full production setup with all hardening (RECOMMENDED for servers)
  ./setup.sh --production --domain oqto.example.com

  # Production with custom SSH port
  ./setup.sh --production --domain oqto.example.com --ssh-port 2222

  # Production but keep password SSH auth (for initial setup)
  ./setup.sh --production --domain oqto.example.com --no-ssh-hardening

  # Multi-user container setup on Linux
  OQTO_USER_MODE=multi OQTO_BACKEND_MODE=container ./setup.sh --production

  # Environment variable style (equivalent to --production)
  OQTO_DEV_MODE=false OQTO_HARDEN_SERVER=yes ./setup.sh --non-interactive
EOF
}

main() {
  NONINTERACTIVE="false"
  FRESH_SETUP="false"

  # Parse arguments
  while [[ $# -gt 0 ]]; do
    case "$1" in
    --help | -h)
      show_help
      exit 0
      ;;
    --update)
      UPDATE_MODE="true"
      shift
      ;;
    --fresh)
      FRESH_SETUP="true"
      rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/oqto/setup-state.env"
      rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/oqto/setup-steps-done"
      shift
      ;;
    --redo)
      # Clear specific steps so they re-run: --redo step1,step2,...
      local redo_steps="${2:-}"
      if [[ -z "$redo_steps" ]]; then
        log_error "--redo requires comma-separated step names"
        log_info "Steps file: ${XDG_CONFIG_HOME:-$HOME/.config}/oqto/setup-steps-done"
        exit 1
      fi
      local steps_file="${XDG_CONFIG_HOME:-$HOME/.config}/oqto/setup-steps-done"
      if [[ -f "$steps_file" ]]; then
        IFS=',' read -ra steps_to_redo <<<"$redo_steps"
        for s in "${steps_to_redo[@]}"; do
          sed -i "/^${s}$/d" "$steps_file" 2>/dev/null || true
          log_info "Cleared step: $s"
        done
      fi
      shift 2
      ;;
    --non-interactive)
      NONINTERACTIVE="true"
      shift
      ;;
    --production | --prod)
      # Production mode with all hardening enabled
      NONINTERACTIVE="true"
      OQTO_DEV_MODE="false"
      OQTO_HARDEN_SERVER="yes"
      OQTO_SETUP_FIREWALL="yes"
      OQTO_SETUP_FAIL2BAN="yes"
      OQTO_HARDEN_SSH="yes"
      OQTO_SETUP_AUTO_UPDATES="yes"
      OQTO_HARDEN_KERNEL="yes"
      OQTO_INSTALL_DEPS="yes"
      OQTO_INSTALL_SERVICE="yes"
      OQTO_INSTALL_AGENT_TOOLS="yes"
      shift
      ;;
    --dev | --development)
      # Development mode, no hardening
      NONINTERACTIVE="true"
      OQTO_DEV_MODE="true"
      OQTO_HARDEN_SERVER="no"
      shift
      ;;
    --domain)
      OQTO_DOMAIN="$2"
      OQTO_SETUP_CADDY="yes"
      shift 2
      ;;
    --domain=*)
      OQTO_DOMAIN="${1#*=}"
      OQTO_SETUP_CADDY="yes"
      shift
      ;;
    --ssh-port)
      OQTO_SSH_PORT="$2"
      shift 2
      ;;
    --ssh-port=*)
      OQTO_SSH_PORT="${1#*=}"
      shift
      ;;
    --no-firewall)
      OQTO_SETUP_FIREWALL="no"
      shift
      ;;
    --no-fail2ban)
      OQTO_SETUP_FAIL2BAN="no"
      shift
      ;;
    --no-ssh-hardening)
      OQTO_HARDEN_SSH="no"
      shift
      ;;
    --no-auto-updates)
      OQTO_SETUP_AUTO_UPDATES="no"
      shift
      ;;
    --no-kernel-hardening)
      OQTO_HARDEN_KERNEL="no"
      shift
      ;;
    --all-tools)
      INSTALL_ALL_TOOLS="true"
      INSTALL_MMRY="true"
      shift
      ;;
    --no-agent-tools)
      OQTO_INSTALL_AGENT_TOOLS="no"
      shift
      ;;
    --config)
      SETUP_CONFIG_FILE="$2"
      shift 2
      ;;
    --config=*)
      SETUP_CONFIG_FILE="${1#*=}"
      shift
      ;;
    *)
      log_error "Unknown option: $1"
      show_help
      exit 1
      ;;
    esac
  done

  # Load config file if specified (oqto.setup.toml)
  if [[ -n "${SETUP_CONFIG_FILE:-}" ]]; then
    if [[ ! -f "$SETUP_CONFIG_FILE" ]]; then
      log_error "Config file not found: $SETUP_CONFIG_FILE"
      exit 1
    fi
    log_info "Loading config from: $SETUP_CONFIG_FILE"
    load_setup_config "$SETUP_CONFIG_FILE"
    NONINTERACTIVE="true"
  fi

  # Apply env vars from web configurator (oqto.dev/setup deploy command)
  if [[ -n "$OQTO_PROVIDERS" ]]; then
    CONFIGURED_PROVIDERS="${OQTO_PROVIDERS//,/ }"
  fi
  if [[ "$OQTO_INSTALL_ALL_TOOLS" == "yes" ]]; then
    INSTALL_ALL_TOOLS="true"
    INSTALL_MMRY="true"
  fi
  if [[ -n "$OQTO_WORKSPACE_DIR" ]]; then
    WORKSPACE_DIR="$OQTO_WORKSPACE_DIR"
  fi
  if [[ -n "$OQTO_ADMIN_USER" ]]; then
    ADMIN_USERNAME="$OQTO_ADMIN_USER"
  fi
  if [[ -n "$OQTO_ADMIN_EMAIL" ]]; then
    ADMIN_EMAIL="$OQTO_ADMIN_EMAIL"
  fi
  # Map OQTO_USER_MODE to internal variable
  if [[ -n "$OQTO_USER_MODE" ]]; then
    SELECTED_USER_MODE="$OQTO_USER_MODE"
  fi
  if [[ -n "$OQTO_BACKEND_MODE" ]]; then
    SELECTED_BACKEND_MODE="$OQTO_BACKEND_MODE"
  fi
  if [[ -n "$OQTO_SETUP_CADDY" && "$OQTO_SETUP_CADDY" == "yes" ]]; then
    SETUP_CADDY="yes"
    if [[ -n "$OQTO_DOMAIN" ]]; then
      DOMAIN="$OQTO_DOMAIN"
    fi
  fi

  echo
  echo -e "${BOLD}${CYAN}"
  cat <<'BANNER'
 ________  ________  _________  ________
|\   __  \|\   __  \|\___   ___\\   __  \
\ \  \|\  \ \  \|\  \|___ \  \_\ \  \|\  \
 \ \  \\\  \ \  \\\  \   \ \  \ \ \  \\\  \
  \ \  \\\  \ \  \\\  \   \ \  \ \ \  \\\  \
   \ \_______\ \_____  \   \ \__\ \ \_______\
    \|_______|\|___| \__\   \|__|  \|_______|
                    \|__|
BANNER
  echo -e "${NC}"
  echo -e "${BOLD}            got tentacles?${NC}"
  echo

  # Save state on exit (including failures) so re-runs can pick up where we left off
  trap save_setup_state EXIT

  # Initialize
  detect_os

  # Update mode: just pull, rebuild, deploy, restart
  if [[ "${UPDATE_MODE:-}" == "true" ]]; then
    update_octo
    return 0
  fi

  # Load previous setup state (if available and not --fresh)
  local use_saved_state="false"
  if [[ "$FRESH_SETUP" != "true" && "$NONINTERACTIVE" != "true" ]]; then
    if load_setup_state; then
      if confirm "Reuse previous setup decisions?"; then
        apply_setup_state
        use_saved_state="true"
      fi
    fi
  elif [[ "$FRESH_SETUP" != "true" && "$NONINTERACTIVE" == "true" ]]; then
    # Non-interactive mode: silently load saved state as defaults
    if [[ -f "$SETUP_STATE_FILE" ]]; then
      apply_setup_state
      use_saved_state="true"
    fi
  fi

  # Mode selection (skip if loaded from state)
  if [[ "$use_saved_state" != "true" ]]; then
    SELECTED_USER_MODE="${OQTO_USER_MODE}"
    SELECTED_BACKEND_MODE="${OQTO_BACKEND_MODE}"
  fi

  # Ensure defaults if state did not include these
  SELECTED_USER_MODE="${SELECTED_USER_MODE:-$OQTO_USER_MODE}"
  SELECTED_BACKEND_MODE="${SELECTED_BACKEND_MODE:-$OQTO_BACKEND_MODE}"

  if [[ "$use_saved_state" != "true" ]]; then
    if [[ "$NONINTERACTIVE" != "true" ]]; then
      select_user_mode
      select_backend_mode
      select_deployment_mode
    else
      # Non-interactive: use env var or default to dev mode
      if [[ -z "$OQTO_DEV_MODE" ]]; then
        OQTO_DEV_MODE="true"
      fi
      PRODUCTION_MODE="$([[ "$OQTO_DEV_MODE" == "false" ]] && echo "true" || echo "false")"
    fi
  fi

  # Prerequisites
  check_prerequisites

  # In multi-user mode, create the oqto system user early
  if [[ "$SELECTED_USER_MODE" == "multi" && "$OS" == "linux" ]]; then
    run_step "octo_system_user" "Oqto system user" ensure_octo_system_user
  fi

  # Install dependencies
  if [[ "$OQTO_INSTALL_DEPS" == "yes" ]]; then
    # Shell tools - verify key binaries exist, not just that the step ran
    verify_or_rerun "shell_tools" "Shell tools" \
      "command -v tmux && command -v rg && (command -v fd || command -v fdfind)" \
      install_shell_tools

    if [[ "$SELECTED_BACKEND_MODE" == "local" ]]; then
      verify_or_rerun "ttyd" "ttyd" "command -v ttyd" install_ttyd
    fi

    # Pi extensions - verify they're actually on disk
    verify_or_rerun "pi_extensions" "Pi extensions" \
      "test -d $HOME/.pi/agent/extensions/oqto-bridge" \
      "$(if [[ "$SELECTED_USER_MODE" == "multi" ]]; then echo install_pi_extensions_all_users; else echo install_pi_extensions; fi)"

    # Agent tools
    if [[ "$OQTO_INSTALL_AGENT_TOOLS" == "yes" ]]; then
      verify_or_rerun "agntz" "agntz" "command -v agntz" install_agntz

      if [[ "$NONINTERACTIVE" != "true" ]] && ! step_done "agent_tools_selected"; then
        select_agent_tools
        mark_step_done "agent_tools_selected"
      fi

      if [[ "$INSTALL_MMRY" == "true" || "$INSTALL_ALL_TOOLS" == "true" ]]; then
        run_step "agent_tools" "Agent tools" install_agent_tools_selected
      fi

      if [[ "$INSTALL_ALL_TOOLS" == "true" ]] || command_exists sx; then
        if ! step_done "searxng"; then
          if confirm "Install SearXNG local search engine for sx?"; then
            install_searxng
            mark_step_done "searxng"
          else
            mark_step_done "searxng"
          fi
        else
          log_success "Already done: SearXNG"
        fi
      fi
    fi
  fi

  # EAVS (LLM proxy)
  verify_or_rerun "eavs_install" "EAVS install" "command -v eavs" install_eavs
  run_step "eavs_configure" "EAVS configure" configure_eavs
  verify_or_rerun "eavs_service" "EAVS service" "systemctl is-enabled eavs 2>/dev/null" install_eavs_service

  # Test providers and generate models.json (after eavs service is running)
  if [[ -n "${CONFIGURED_PROVIDERS:-}" ]]; then
    run_step "eavs_test" "EAVS provider tests" test_eavs_providers
  fi
  run_step "eavs_models" "EAVS models.json" generate_eavs_models_json

  # Build Oqto - ALWAYS rebuild to ensure binaries match the current source.
  # This is critical: stale binaries cause subtle bugs that are hard to diagnose.
  # The build is incremental (cargo only recompiles changed crates) so it's fast
  # when nothing changed.
  run_step_always "build_octo" "Build Oqto" build_octo || {
    log_error "Build failed. Cannot continue without binaries."
    exit 1
  }

  # Generate configuration
  run_step "generate_config" "Configuration" generate_config

  # Onboarding templates and shared repos
  run_step "onboarding_templates" "Onboarding templates" setup_onboarding_templates_repo
  run_step "external_repos" "External repos" update_external_repos
  run_step "feedback_dirs" "Feedback directories" setup_feedback_dirs

  # Linux user isolation
  verify_or_rerun "linux_user_isolation" "Linux user isolation" "test -f /etc/sudoers.d/oqto-multiuser" setup_linux_user_isolation

  # Container image
  run_step "container_image" "Container image" build_container_image

  # Caddy reverse proxy
  if [[ "$SETUP_CADDY" == "yes" ]]; then
    verify_or_rerun "caddy_install" "Caddy install" "command -v caddy" install_caddy
    # Verify Caddyfile contains our config, not the default
    verify_or_rerun "caddy_config" "Caddy config" \
      "grep -q 'reverse_proxy' /etc/caddy/Caddyfile 2>/dev/null" \
      generate_caddyfile
    verify_or_rerun "caddy_service" "Caddy service" "systemctl is-enabled caddy 2>/dev/null" install_caddy_service
  fi

  # Server hardening
  run_step "harden_server" "Server hardening" harden_server

  # Install service
  run_step_always "install_service" "System service" install_service || {
    log_error "Service installation failed. Cannot start services."
    exit 1
  }

  # Create admin user in database
  run_step "admin_user_db" "Admin user in database" create_admin_user_db

  # Start all services
  start_all_services

  # Summary
  print_summary
}

main "$@"
