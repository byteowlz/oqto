# ==============================================================================
# Configuration and Defaults
# ==============================================================================

# SETUP_DIR is set by the main setup.sh before sourcing modules
SCRIPT_DIR="${SETUP_DIR:?SETUP_DIR must be set by setup.sh}"
TEMPLATES_DIR="${SCRIPT_DIR}/templates"
# Use SSH for private repos (allows SSH key authentication)
ONBOARDING_TEMPLATES_REPO_DEFAULT="https://github.com/byteowlz/oqto-templates.git"
EXTERNAL_REPOS_DIR_DEFAULT="/usr/local/share/oqto/external-repos"
ONBOARDING_TEMPLATES_PATH_DEFAULT="/usr/share/oqto/oqto-templates/"
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

# Voice services (eaRS STT + kokorox TTS)
: "${VOICE_ENABLED:=false}"           # true or false
: "${VOICE_STT_URL:=ws://127.0.0.1:8765}"  # eaRS WebSocket URL
: "${VOICE_TTS_URL:=ws://127.0.0.1:8766}"  # kokorox WebSocket URL

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

# Playwright browser path (single source of truth for all tools)
# Single-user: user-local; Multi-user: system-wide (set in build_octo)
BROWSERD_DEPLOY_DIR=""
PW_BROWSERS_DIR=""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

