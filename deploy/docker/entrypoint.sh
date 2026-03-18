#!/usr/bin/env bash
# =============================================================================
# Oqto All-in-One Entrypoint
# =============================================================================
#
# Starts all services in the correct dependency order:
#   1. hstry   (chat history, gRPC)
#   2. eavs    (LLM proxy)
#   3. oqto    (backend API + WebSocket)
#   4. caddy   (reverse proxy, serves frontend)
#
# Environment variables:
#   Required:
#     ANTHROPIC_API_KEY    - Anthropic API key (or other provider keys)
#     JWT_SECRET           - JWT signing secret (min 32 chars, auto-generated if unset)
#
#   Optional (providers):
#     OPENAI_API_KEY       - OpenAI API key
#     GEMINI_API_KEY       - Google Gemini API key
#     OPENROUTER_API_KEY   - OpenRouter API key
#
#   Optional (configuration):
#     OQTO_PORT            - External port (default: 8080)
#     EAVS_PORT            - Eavs internal port (default: 3033)
#     ADMIN_USER           - Bootstrap admin username (default: admin)
#     ADMIN_PASSWORD       - Bootstrap admin password (auto-generated if unset)
#     ADMIN_EMAIL          - Bootstrap admin email (default: admin@oqto.local)
#     OQTO_LOG_LEVEL       - Log level: error/warn/info/debug/trace (default: info)
#     OQTO_DATA_DIR        - Data directory (default: /data)
#     OQTO_SINGLE_USER     - Single-user mode, skip auth (default: false)
#
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

OQTO_DATA_DIR="${OQTO_DATA_DIR:-/data}"
OQTO_PORT="${OQTO_PORT:-8080}"
EAVS_PORT="${EAVS_PORT:-3033}"
OQTO_LOG_LEVEL="${OQTO_LOG_LEVEL:-info}"
OQTO_SINGLE_USER="${OQTO_SINGLE_USER:-false}"

ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@oqto.local}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-}"
JWT_SECRET="${JWT_SECRET:-}"

# Derived paths
HSTRY_DB="${OQTO_DATA_DIR}/hstry/hstry.db"
EAVS_DATA="${OQTO_DATA_DIR}/eavs"
OQTO_DB="${OQTO_DATA_DIR}/oqto/oqto.db"
OQTO_USERS="${OQTO_DATA_DIR}/users"
OQTO_WORKSPACES="${OQTO_DATA_DIR}/workspaces"
RUNNER_SOCKET="/run/oqto/runner.sock"
EAVS_CONFIG_DIR="${OQTO_DATA_DIR}/eavs"

PIDS=()

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log() { echo "[oqto-init] $(date '+%H:%M:%S') $*"; }
log_error() { echo "[oqto-init] $(date '+%H:%M:%S') ERROR: $*" >&2; }

generate_secret() {
  head -c 48 /dev/urandom | base64 | tr -d '/+=' | head -c "${1:-64}"
}

wait_for_port() {
  local port="$1" name="$2" timeout="${3:-30}"
  local waited=0
  while ! curl -sf "http://127.0.0.1:${port}/health" >/dev/null 2>&1; do
    if [ "$waited" -ge "$timeout" ]; then
      log_error "${name} did not become healthy on port ${port} within ${timeout}s"
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  log "${name} is ready on port ${port} (${waited}s)"
}

wait_for_hstry() {
  local timeout="${1:-30}" waited=0
  # hstry uses gRPC, not HTTP -- just check if the process is alive
  # and the port file exists
  while [ "$waited" -lt "$timeout" ]; do
    if [ -f "${HOME}/.local/state/hstry/port" ] || \
       [ -S "${HOME}/.local/state/hstry/service.sock" ]; then
      log "hstry is ready (${waited}s)"
      return 0
    fi
    sleep 1
    waited=$((waited + 1))
  done
  # If the process is still alive, assume it's fine
  log "hstry startup wait complete (may still be initializing)"
}

cleanup() {
  log "Shutting down services..."
  for pid in "${PIDS[@]}"; do
    kill "$pid" 2>/dev/null || true
  done
  wait 2>/dev/null || true
  log "Shutdown complete."
  exit 0
}

trap cleanup SIGTERM SIGINT EXIT

# ---------------------------------------------------------------------------
# Directory setup
# ---------------------------------------------------------------------------

log "Initializing Oqto (data: ${OQTO_DATA_DIR})"

mkdir -p \
  "${OQTO_DATA_DIR}/hstry" \
  "${OQTO_DATA_DIR}/eavs" \
  "${OQTO_DATA_DIR}/oqto" \
  "${OQTO_DATA_DIR}/users" \
  "${OQTO_DATA_DIR}/workspaces" \
  /run/oqto \
  /home/oqto/.config/hstry \
  /home/oqto/.config/eavs \
  /home/oqto/.local/state/hstry \
  /home/oqto/.local/share/hstry \
  /home/oqto/.pi/agent/sessions

chown -R oqto:oqto "${OQTO_DATA_DIR}" /run/oqto /home/oqto 2>/dev/null || true

# ---------------------------------------------------------------------------
# Generate secrets if not provided
# ---------------------------------------------------------------------------

if [ -z "$JWT_SECRET" ]; then
  # Persist generated secret so it survives container restarts (with volume)
  JWT_SECRET_FILE="${OQTO_DATA_DIR}/oqto/.jwt_secret"
  if [ -f "$JWT_SECRET_FILE" ]; then
    JWT_SECRET=$(cat "$JWT_SECRET_FILE")
    log "Using persisted JWT secret"
  else
    JWT_SECRET=$(generate_secret 64)
    echo "$JWT_SECRET" > "$JWT_SECRET_FILE"
    chmod 600 "$JWT_SECRET_FILE"
    log "Generated and persisted JWT secret"
  fi
fi

EAVS_ADMIN_KEY_FILE="${OQTO_DATA_DIR}/eavs/.admin_key"
if [ -f "$EAVS_ADMIN_KEY_FILE" ]; then
  EAVS_ADMIN_KEY=$(cat "$EAVS_ADMIN_KEY_FILE")
else
  EAVS_ADMIN_KEY=$(generate_secret 32)
  echo "$EAVS_ADMIN_KEY" > "$EAVS_ADMIN_KEY_FILE"
  chmod 600 "$EAVS_ADMIN_KEY_FILE"
  log "Generated eavs admin key"
fi

# ---------------------------------------------------------------------------
# Write eavs config
# ---------------------------------------------------------------------------

EAVS_ENV_FILE="${EAVS_CONFIG_DIR}/eavs.env"
EAVS_CONFIG_FILE="/home/oqto/.config/eavs/config.toml"

# Build eavs env file from provided API keys
cat > "$EAVS_ENV_FILE" <<EOF
EAVS_PORT=${EAVS_PORT}
EAVS_ADMIN_KEY=${EAVS_ADMIN_KEY}
EOF

# Append provider keys if set
[ -n "${ANTHROPIC_API_KEY:-}" ]    && echo "ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${OPENAI_API_KEY:-}" ]       && echo "OPENAI_API_KEY=${OPENAI_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${GEMINI_API_KEY:-}" ]       && echo "GEMINI_API_KEY=${GEMINI_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${OPENROUTER_API_KEY:-}" ]   && echo "OPENROUTER_API_KEY=${OPENROUTER_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${AZURE_API_KEY:-}" ]        && echo "AZURE_API_KEY=${AZURE_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${DEEPSEEK_API_KEY:-}" ]     && echo "DEEPSEEK_API_KEY=${DEEPSEEK_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${MISTRAL_API_KEY:-}" ]      && echo "MISTRAL_API_KEY=${MISTRAL_API_KEY}" >> "$EAVS_ENV_FILE"

chmod 600 "$EAVS_ENV_FILE"

# Write minimal eavs config.toml
mkdir -p /home/oqto/.config/eavs
cat > "$EAVS_CONFIG_FILE" <<EOF
[server]
host = "127.0.0.1"
port = ${EAVS_PORT}

[admin]
api_key = "${EAVS_ADMIN_KEY}"
EOF

# Add provider sections based on available keys
if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
  cat >> "$EAVS_CONFIG_FILE" <<EOF

[providers.anthropic]
type = "anthropic"
api_key = "env:ANTHROPIC_API_KEY"
EOF
fi

if [ -n "${OPENAI_API_KEY:-}" ]; then
  cat >> "$EAVS_CONFIG_FILE" <<EOF

[providers.openai]
type = "openai"
api_key = "env:OPENAI_API_KEY"
EOF
fi

if [ -n "${GEMINI_API_KEY:-}" ]; then
  cat >> "$EAVS_CONFIG_FILE" <<EOF

[providers.google]
type = "google"
api_key = "env:GEMINI_API_KEY"
EOF
fi

if [ -n "${OPENROUTER_API_KEY:-}" ]; then
  cat >> "$EAVS_CONFIG_FILE" <<EOF

[providers.openrouter]
type = "openai"
api_key = "env:OPENROUTER_API_KEY"
base_url = "https://openrouter.ai/api/v1"
EOF
fi

chown -R oqto:oqto /home/oqto/.config/eavs

# ---------------------------------------------------------------------------
# Write hstry config
# ---------------------------------------------------------------------------

cat > /home/oqto/.config/hstry/config.toml <<EOF
database = "${HSTRY_DB}"
adapter_paths = ["/usr/local/share/hstry/adapters"]
js_runtime = "bun"

[service]
enabled = true
poll_interval_secs = 30
search_api = true
transport = "tcp"
EOF

chown -R oqto:oqto /home/oqto/.config/hstry

# ---------------------------------------------------------------------------
# Write oqto config
# ---------------------------------------------------------------------------

AUTH_MODE="multi"
if [ "$OQTO_SINGLE_USER" = "true" ]; then
  AUTH_MODE="none"
fi

cat > /etc/oqto/config.toml <<EOF
[server]
host = "127.0.0.1"
port = 8081

[auth]
mode = "${AUTH_MODE}"

[auth.jwt]
secret = "${JWT_SECRET}"
expiry = "7d"

[database]
path = "${OQTO_DB}"

[storage]
data_dir = "${OQTO_USERS}"

[hstry]
# oqto discovers hstry via port file / unix socket automatically

[eavs]
url = "http://127.0.0.1:${EAVS_PORT}"
admin_key = "${EAVS_ADMIN_KEY}"

[runner]
mode = "local"

[runner.local]
fileserver_binary = "oqto-files"
ttyd_binary = "ttyd"
workspace_dir = "${OQTO_WORKSPACES}/{user_id}"
single_user = ${OQTO_SINGLE_USER}

[features]
voice = false
files = true
terminal = true

[logging]
level = "${OQTO_LOG_LEVEL}"
format = "pretty"
EOF

chown oqto:oqto /etc/oqto/config.toml
chmod 600 /etc/oqto/config.toml

# ---------------------------------------------------------------------------
# Write Caddyfile
# ---------------------------------------------------------------------------

cat > /etc/caddy/Caddyfile <<EOF
:${OQTO_PORT} {
    # Health check
    handle /health {
        respond "OK" 200
    }

    # API and WebSocket -> oqto backend
    handle /api/* {
        reverse_proxy 127.0.0.1:8081
    }

    handle /ws/* {
        reverse_proxy 127.0.0.1:8081
    }

    @websockets {
        header Connection *Upgrade*
        header Upgrade websocket
    }
    handle @websockets {
        reverse_proxy 127.0.0.1:8081
    }

    # Frontend static files
    handle {
        root * /usr/local/share/oqto/frontend
        try_files {path} /index.html
        file_server
    }

    header {
        X-Content-Type-Options nosniff
        X-Frame-Options SAMEORIGIN
        Referrer-Policy strict-origin-when-cross-origin
        -Server
    }

    encode gzip zstd
}
EOF

# ---------------------------------------------------------------------------
# Start services
# ---------------------------------------------------------------------------

# 1. hstry (chat history)
log "Starting hstry..."
su -s /bin/bash oqto -c "
  export HOME=/home/oqto
  export XDG_CONFIG_HOME=/home/oqto/.config
  export XDG_DATA_HOME=/home/oqto/.local/share
  export XDG_STATE_HOME=/home/oqto/.local/state
  hstry service run 2>&1 | sed 's/^/[hstry] /'
" &
PIDS+=($!)
wait_for_hstry 15

# 2. eavs (LLM proxy)
log "Starting eavs..."
# Source env file so provider keys are in the environment
set -a
# shellcheck source=/dev/null
source "$EAVS_ENV_FILE"
set +a
su -s /bin/bash oqto -c "
  export HOME=/home/oqto
  export XDG_CONFIG_HOME=/home/oqto/.config
  $(env | grep -E '^(ANTHROPIC|OPENAI|GEMINI|OPENROUTER|AZURE|DEEPSEEK|MISTRAL|EAVS_)' | sed 's/^/export /')
  eavs serve --port ${EAVS_PORT} --host 127.0.0.1 2>&1 | sed 's/^/[eavs] /'
" &
PIDS+=($!)
wait_for_port "$EAVS_PORT" "eavs" 15

# 3. oqto backend
log "Starting oqto backend..."
su -s /bin/bash oqto -c "
  export HOME=/home/oqto
  export OQTO_CONFIG=/etc/oqto/config.toml
  export XDG_CONFIG_HOME=/home/oqto/.config
  export XDG_DATA_HOME=/home/oqto/.local/share
  export XDG_STATE_HOME=/home/oqto/.local/state
  export XDG_RUNTIME_DIR=/run/oqto
  oqto serve --config /etc/oqto/config.toml 2>&1 | sed 's/^/[oqto] /'
" &
PIDS+=($!)
wait_for_port 8081 "oqto" 15

# 4. Bootstrap admin user (first run only)
if [ ! -f "${OQTO_DATA_DIR}/oqto/.bootstrapped" ] && [ "$AUTH_MODE" = "multi" ]; then
  if [ -z "$ADMIN_PASSWORD" ]; then
    ADMIN_PASSWORD=$(generate_secret 16)
    log "============================================"
    log "  Generated admin credentials:"
    log "    Username: ${ADMIN_USER}"
    log "    Password: ${ADMIN_PASSWORD}"
    log "  Save these! They won't be shown again."
    log "============================================"
  fi

  log "Bootstrapping admin user..."
  su -s /bin/bash oqto -c "
    export HOME=/home/oqto
    oqtoctl user bootstrap \
      -u '${ADMIN_USER}' \
      -e '${ADMIN_EMAIL}' \
      -p '${ADMIN_PASSWORD}' \
      --no-linux-user \
      --database '${OQTO_DB}' 2>&1
  " && touch "${OQTO_DATA_DIR}/oqto/.bootstrapped" \
    || log_error "Admin bootstrap failed (may already exist)"
fi

# 5. caddy (reverse proxy + frontend)
log "Starting caddy..."
caddy run --config /etc/caddy/Caddyfile --adapter caddyfile 2>&1 | sed 's/^/[caddy] /' &
PIDS+=($!)

log "============================================"
log "  Oqto is running!"
log "  URL: http://localhost:${OQTO_PORT}"
log "============================================"

# Wait for any process to exit
wait -n "${PIDS[@]}" 2>/dev/null || true

# If any process exits, log which one and shut down
for i in "${!PIDS[@]}"; do
  if ! kill -0 "${PIDS[$i]}" 2>/dev/null; then
    log_error "Service (PID ${PIDS[$i]}) exited unexpectedly"
  fi
done

# Trigger cleanup
exit 1
