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
#     At least one LLM provider key (ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.)
#
#   Optional:
#     JWT_SECRET           - JWT signing secret (auto-generated + persisted if unset)
#     ADMIN_USER           - Bootstrap admin username (default: admin)
#     ADMIN_PASSWORD       - Bootstrap admin password (auto-generated if unset)
#     ADMIN_EMAIL          - Bootstrap admin email (default: admin@oqto.local)
#     OQTO_PORT            - External port (default: 8080)
#     EAVS_PORT            - Eavs internal port (default: 3033)
#     OQTO_LOG_LEVEL       - Log level: error/warn/info/debug/trace (default: info)
#     OQTO_DATA_DIR        - Data directory (default: /data)
#     OQTO_SINGLE_USER     - Single-user dev mode, bypass auth (default: false)
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

# Internal ports (not exposed)
OQTO_BACKEND_PORT=8081

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
  local port="$1" name="$2" timeout="${3:-30}" path="${4:-/health}"
  local waited=0
  while ! curl -sf "http://127.0.0.1:${port}${path}" >/dev/null 2>&1; do
    if [ "$waited" -ge "$timeout" ]; then
      log_error "${name} did not become healthy on port ${port}${path} within ${timeout}s"
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  log "${name} is ready on port ${port} (${waited}s)"
}

wait_for_hstry() {
  local timeout="${1:-30}" waited=0
  while [ "$waited" -lt "$timeout" ]; do
    if [ -f "/home/oqto/.local/state/hstry/port" ] || \
       [ -S "/home/oqto/.local/state/hstry/service.sock" ]; then
      log "hstry is ready (${waited}s)"
      return 0
    fi
    sleep 1
    waited=$((waited + 1))
  done
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
  /home/oqto/.config/oqto \
  /home/oqto/.config/hstry \
  /home/oqto/.config/eavs \
  /home/oqto/.local/state/hstry \
  /home/oqto/.local/share/hstry \
  "${OQTO_DATA_DIR}/pi-sessions"

chown -R oqto:oqto "${OQTO_DATA_DIR}" /run/oqto /home/oqto 2>/dev/null || true

# Symlink Pi sessions into the persistent volume so they survive container rebuilds
mkdir -p /home/oqto/.pi/agent
ln -sfn "${OQTO_DATA_DIR}/pi-sessions" /home/oqto/.pi/agent/sessions

# ---------------------------------------------------------------------------
# Generate secrets if not provided
# ---------------------------------------------------------------------------

if [ -z "$JWT_SECRET" ]; then
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

EAVS_ENV_FILE="${OQTO_DATA_DIR}/eavs/eavs.env"

cat > "$EAVS_ENV_FILE" <<EOF
EAVS_PORT=${EAVS_PORT}
EAVS_ADMIN_KEY=${EAVS_ADMIN_KEY}
EOF

[ -n "${ANTHROPIC_API_KEY:-}" ]    && echo "ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${OPENAI_API_KEY:-}" ]       && echo "OPENAI_API_KEY=${OPENAI_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${GEMINI_API_KEY:-}" ]       && echo "GEMINI_API_KEY=${GEMINI_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${OPENROUTER_API_KEY:-}" ]   && echo "OPENROUTER_API_KEY=${OPENROUTER_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${AZURE_API_KEY:-}" ]        && echo "AZURE_API_KEY=${AZURE_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${DEEPSEEK_API_KEY:-}" ]     && echo "DEEPSEEK_API_KEY=${DEEPSEEK_API_KEY}" >> "$EAVS_ENV_FILE"
[ -n "${MISTRAL_API_KEY:-}" ]      && echo "MISTRAL_API_KEY=${MISTRAL_API_KEY}" >> "$EAVS_ENV_FILE"

chmod 600 "$EAVS_ENV_FILE"

# Write eavs config.toml
EAVS_CONFIG_FILE="/home/oqto/.config/eavs/config.toml"
cat > "$EAVS_CONFIG_FILE" <<EOF
[server]
host = "127.0.0.1"
port = ${EAVS_PORT}

[admin]
api_key = "${EAVS_ADMIN_KEY}"
EOF

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

# Local LLM (OpenAI-compatible: Ollama, LM Studio, llama.cpp, vLLM, etc.)
if [ -n "${LOCAL_LLM_URL:-}" ]; then
  cat >> "$EAVS_CONFIG_FILE" <<EOF

[providers.local]
type = "openai-compatible"
base_url = "${LOCAL_LLM_URL}/v1"
EOF
  # Strip trailing /v1 if the user already included it
  sed -i 's|/v1/v1|/v1|g' "$EAVS_CONFIG_FILE"
fi

chown -R oqto:oqto /home/oqto/.config/eavs

# ---------------------------------------------------------------------------
# Write hstry config
# ---------------------------------------------------------------------------

cat > /home/oqto/.config/hstry/config.toml <<EOF
database = "${OQTO_DATA_DIR}/hstry/hstry.db"
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
# Write oqto config (matches AppConfig struct)
# ---------------------------------------------------------------------------

DEV_MODE="false"
if [ "$OQTO_SINGLE_USER" = "true" ]; then
  DEV_MODE="true"
fi

cat > /home/oqto/.config/oqto/config.toml <<EOF
# Oqto Docker All-in-One configuration
# Generated by entrypoint.sh -- do not edit manually

profile = "docker"

[logging]
level = "${OQTO_LOG_LEVEL}"
format = "pretty"

[runtime]

[paths]
data_dir = "${OQTO_DATA_DIR}/oqto"

[backend]
mode = "local"

[local]
enabled = true
fileserver_binary = "oqto-files"
ttyd_binary = "ttyd"
workspace_dir = "${OQTO_DATA_DIR}/workspaces/{user_id}"
single_user = ${OQTO_SINGLE_USER}
cleanup_on_startup = true
stop_sessions_on_shutdown = true

[local.linux_users]
enabled = true
create_home = true
use_sudo = true

[auth]
dev_mode = ${DEV_MODE}
jwt_secret = "${JWT_SECRET}"

[eavs]
enabled = true
base_url = "http://127.0.0.1:${EAVS_PORT}"
master_key = "${EAVS_ADMIN_KEY}"

[hstry]

[voice]
enabled = false

[sessions]

[mmry]

[sldr]

[server]
admin_socket_path = "/run/oqto/oqtoctl.sock"

[container]

[templates]

[scaffold]

[pi]

[agent_browser]

[onboarding_templates]

[feedback]
EOF

chown -R oqto:oqto /home/oqto/.config/oqto
chmod 600 /home/oqto/.config/oqto/config.toml

# ---------------------------------------------------------------------------
# Write Caddyfile
# ---------------------------------------------------------------------------

cat > /etc/caddy/Caddyfile <<EOF
:${OQTO_PORT} {
    handle /health {
        respond "OK" 200
    }

    # API and WebSocket -> oqto backend
    handle /api/* {
        reverse_proxy 127.0.0.1:${OQTO_BACKEND_PORT}
    }

    handle /ws/* {
        reverse_proxy 127.0.0.1:${OQTO_BACKEND_PORT}
    }

    @websockets {
        header Connection *Upgrade*
        header Upgrade websocket
    }
    handle @websockets {
        reverse_proxy 127.0.0.1:${OQTO_BACKEND_PORT}
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
  hstry adapters update 2>&1 || true
  hstry service run 2>&1 | sed 's/^/[hstry] /'
" &
PIDS+=($!)
wait_for_hstry 15

# 2. eavs (LLM proxy)
log "Starting eavs..."
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

# Test if oqto binary works at all
su -s /bin/bash oqto -c "oqto --help" >/dev/null 2>&1 \
  || { log_error "oqto binary failed to execute"; su -s /bin/bash oqto -c "oqto --help" 2>&1; }

# Dump generated config for debugging
log "Config file:"
cat /home/oqto/.config/oqto/config.toml | sed 's/^/  /'

OQTO_LOG="/tmp/oqto.log"
su -s /bin/bash oqto -c "
  export HOME=/home/oqto
  export XDG_CONFIG_HOME=/home/oqto/.config
  export XDG_DATA_HOME=/home/oqto/.local/share
  export XDG_STATE_HOME=/home/oqto/.local/state
  export XDG_RUNTIME_DIR=/run/oqto
  export RUST_LOG=${OQTO_LOG_LEVEL:-info}
  oqto --config /home/oqto/.config/oqto/config.toml serve \
    --local-mode \
    --host 0.0.0.0 \
    --port ${OQTO_BACKEND_PORT} \
    --user-data-path ${OQTO_DATA_DIR}/users \
    >$OQTO_LOG 2>&1
" &
PIDS+=($!)

# Tail the log in background so we see output
tail -f "$OQTO_LOG" 2>/dev/null | sed 's/^/[oqto] /' &

# Wait for oqto, dump log on failure
if ! wait_for_port "$OQTO_BACKEND_PORT" "oqto" 30 "/api/health"; then
  log_error "oqto startup log:"
  cat "$OQTO_LOG" 2>/dev/null | sed 's/^/  /' || true
  exit 1
fi

# 4. Bootstrap admin user (first run only)
if [ ! -f "${OQTO_DATA_DIR}/oqto/.bootstrapped" ] && [ "$DEV_MODE" = "false" ]; then
  if [ -z "$ADMIN_PASSWORD" ]; then
    ADMIN_PASSWORD=$(generate_secret 16)
    log "============================================"
    log "  Admin credentials:"
    log "    Username: ${ADMIN_USER}"
    log "    Password: ${ADMIN_PASSWORD}"
    log "  Save these! They won't be shown again."
    log "============================================"
  fi

  log "Creating admin user..."
  su -s /bin/bash oqto -c "
    export HOME=/home/oqto
    export XDG_RUNTIME_DIR=/run/oqto
    oqtoctl user create \
      '${ADMIN_USER}' \
      -e '${ADMIN_EMAIL}' \
      -p '${ADMIN_PASSWORD}' \
      -r admin \
      2>&1
  " && log "Admin user created" \
    || log_error "Admin user creation failed"

  touch "${OQTO_DATA_DIR}/oqto/.bootstrapped"
fi

# 5. caddy (reverse proxy + frontend)
log "Starting caddy..."
caddy run --config /etc/caddy/Caddyfile --adapter caddyfile 2>&1 | sed 's/^/[caddy] /' &
PIDS+=($!)

log "============================================"
log "  Oqto is running!"
log "  URL: http://localhost:${OQTO_HOST_PORT:-${OQTO_PORT}}"
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
