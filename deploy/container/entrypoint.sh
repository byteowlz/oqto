#!/usr/bin/env bash
set -e

HOME_DIR="${HOME:-/home/dev}"

# Default ports
OPENCODE_PORT="${OPENCODE_PORT:-41820}"
FILESERVER_PORT="${FILESERVER_PORT:-41821}"
TTYD_PORT="${TTYD_PORT:-41822}"
MMRY_PORT="${MMRY_PORT:-41823}"
PI_BRIDGE_PORT="${PI_BRIDGE_PORT:-41824}"
WORKSPACE_DIR="${WORKSPACE_DIR:-${HOME_DIR}/workspace}"
SKEL_DIR="${SKEL_DIR:-/usr/local/share/skel}"
OPENCODE_SEED_DIR="${OPENCODE_SEED_DIR:-/opt/opencode}"
OPENCODE_BIN="${OPENCODE_BIN:-opencode}"
XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME_DIR}/.config}"

# EAVS configuration (host-based proxy)
# EAVS_URL - URL to the host EAVS proxy (e.g., http://host.docker.internal:41800)
# EAVS_VIRTUAL_KEY - Virtual key for this session (created by backend)

# MMRY configuration (host-based embeddings)
# MMRY_HOST_URL - URL to host mmry service for embeddings (e.g., http://host.docker.internal:8081)
# MMRY_HOST_KEY - API key for authenticating with host mmry service

echo "Starting OpenCode development container..."
echo "OpenCode server port: ${OPENCODE_PORT}"
echo "File server port: ${FILESERVER_PORT}"
echo "TTY terminal port: ${TTYD_PORT}"
echo "mmry service port: ${MMRY_PORT}"
echo "Pi bridge port: ${PI_BRIDGE_PORT}"
echo "Workspace directory: ${WORKSPACE_DIR}"
if [ -n "${EAVS_URL}" ]; then
    echo "EAVS proxy URL: ${EAVS_URL}"
fi
if [ -n "${MMRY_HOST_URL}" ]; then
    echo "mmry host URL: ${MMRY_HOST_URL}"
fi

# Initialize home with shipped defaults.
if [ -d "${SKEL_DIR}" ]; then
    # Force-update system directories (templates, brands) to get latest versions
    for sys_dir in ".local/share/tmpltr"; do
        if [ -d "${SKEL_DIR}/${sys_dir}" ]; then
            mkdir -p "${HOME_DIR}/${sys_dir}"
            cp -a "${SKEL_DIR}/${sys_dir}/." "${HOME_DIR}/${sys_dir}/" 2>/dev/null || \
                echo "Warning: failed to update ${sys_dir}"
        fi
    done
    # No-clobber copy other files to preserve user customizations (.zshrc, etc)
    if ! cp -an "${SKEL_DIR}/." "${HOME_DIR}/" 2>/dev/null; then
        echo "Warning: failed to copy defaults from ${SKEL_DIR} into ${HOME_DIR} (check volume permissions)"
    fi
fi

# Ensure XDG directories exist (may be missing on first run when /home is mounted).
mkdir -p "${XDG_CONFIG_HOME}" "${HOME_DIR}/.local/share" "${HOME_DIR}/.local/state" "${HOME_DIR}/.cache"

# Ensure opencode is available even when /home is mounted over the image layer.
# Always update the opencode binaries from the seed directory to get the latest version.
if [ -d "${OPENCODE_SEED_DIR}" ]; then
    mkdir -p "${HOME_DIR}/.opencode"
    # Force-copy bin directory to ensure we have the latest opencode binary
    if [ -d "${OPENCODE_SEED_DIR}/bin" ]; then
        cp -a "${OPENCODE_SEED_DIR}/bin" "${HOME_DIR}/.opencode/" 2>/dev/null || \
            echo "Warning: failed to update opencode binaries"
    fi
    # No-clobber copy other files (config, etc) to preserve user customizations
    if ! cp -an "${OPENCODE_SEED_DIR}/." "${HOME_DIR}/.opencode/" 2>/dev/null; then
        echo "Warning: failed to seed ${HOME_DIR}/.opencode from ${OPENCODE_SEED_DIR} (check volume permissions)"
    fi
fi

# Ensure workspace directory exists
mkdir -p "${WORKSPACE_DIR}"
cd "${WORKSPACE_DIR}"

# Configure opencode to use EAVS proxy if available
if [ -n "${EAVS_URL}" ] && [ -n "${EAVS_VIRTUAL_KEY}" ]; then
    echo "Configuring OpenCode to use EAVS proxy..."
    mkdir -p "${XDG_CONFIG_HOME}/opencode"
    cat > "${XDG_CONFIG_HOME}/opencode/opencode.json" <<EOF
{
  "provider": {
    "anthropic": {
      "baseURL": "${EAVS_URL}/v1"
    },
    "openai": {
      "baseURL": "${EAVS_URL}/v1"
    }
  }
}
EOF
    # Set the virtual key as the API key for providers
    export ANTHROPIC_API_KEY="${EAVS_VIRTUAL_KEY}"
    export OPENAI_API_KEY="${EAVS_VIRTUAL_KEY}"
fi

# Function to cleanup background processes on exit
cleanup() {
    echo "Shutting down services..."
    kill $(jobs -p) 2>/dev/null || true
    exit 0
}
trap cleanup SIGTERM SIGINT

# Start file server in the background
echo "Starting file server on port ${FILESERVER_PORT}..."
if command -v fileserver &> /dev/null; then
    # Use our custom Rust fileserver
    fileserver --port "${FILESERVER_PORT}" --bind 0.0.0.0 --root "${WORKSPACE_DIR}" &
else
    # Fallback to Python http.server (limited functionality)
    echo "Warning: fileserver binary not found, using Python fallback (no /tree or upload support)"
    if command -v python &> /dev/null; then
        python -m http.server "${FILESERVER_PORT}" --bind 0.0.0.0 --directory "${WORKSPACE_DIR}" &
    else
        python3 -m http.server "${FILESERVER_PORT}" --bind 0.0.0.0 --directory "${WORKSPACE_DIR}" &
    fi
fi
FILE_SERVER_PID=$!

# Start ttyd web terminal in the background
echo "Starting ttyd terminal on port ${TTYD_PORT}..."
ttyd \
    --port "${TTYD_PORT}" \
    --interface 0.0.0.0 \
    --writable \
    --cwd "${WORKSPACE_DIR}" \
    zsh -l &
TTYD_PID=$!

# Configure and start mmry service (lean mode, embeddings delegated to host)
if command -v mmry &> /dev/null; then
    echo "Starting mmry service on port ${MMRY_PORT}..."
    
    # Create mmry config directory
    MMRY_CONFIG_DIR="${XDG_CONFIG_HOME}/mmry"
    mkdir -p "${MMRY_CONFIG_DIR}"
    
    # Generate mmry config that delegates embeddings to host
    cat > "${MMRY_CONFIG_DIR}/config.toml" <<EOF
# mmry configuration for container mode
# Embeddings are delegated to host mmry service

[service]
enabled = true
auto_start = false

[external_api]
enable = true
host = "0.0.0.0"
port = ${MMRY_PORT}
require_api_key = false

[embeddings]
enabled = true
model = "nomic-ai/nomic-embed-text-v1.5"
dimension = 768

# Delegate embeddings to host service
[embeddings.remote]
base_url = "${MMRY_HOST_URL:-http://host.containers.internal:8081}"
api_key = "${MMRY_HOST_KEY:-}"
request_timeout_seconds = 30
max_batch_size = 64
required = false

[search]
rerank_enabled = true

[search.remote_rerank]
base_url = "${MMRY_HOST_URL:-http://host.containers.internal:8081}"
api_key = "${MMRY_HOST_KEY:-}"
request_timeout_seconds = 30
max_batch_size = 64
required = false

[hmlr]
enabled = false

[sparse_embeddings]
enabled = false
EOF
    
    # Initialize mmry database if needed
    mmry init 2>/dev/null || true
    
    # Start mmry service in background
    mmry service run &
    MMRY_PID=$!
fi

# Start pi-bridge if enabled and both pi-bridge and Pi CLI are available
# PI_BRIDGE_ENABLED is set by the orchestrator when Main Chat uses container runtime
# PI_EXECUTABLE can be set to override the Pi CLI path (default: "pi")
PI_EXECUTABLE="${PI_EXECUTABLE:-pi}"
if [ "${PI_BRIDGE_ENABLED:-false}" = "true" ]; then
    if command -v pi-bridge &> /dev/null && command -v "${PI_EXECUTABLE}" &> /dev/null; then
        echo "Starting pi-bridge on port ${PI_BRIDGE_PORT}..."
        
        # Pi-bridge will spawn the Pi CLI in RPC mode
        # Work directory is the workspace so Pi has access to project files
        pi-bridge \
            --port "${PI_BRIDGE_PORT}" \
            --host 0.0.0.0 \
            --work-dir "${WORKSPACE_DIR}" \
            --pi-executable "${PI_EXECUTABLE}" \
            ${PI_PROVIDER:+--provider "${PI_PROVIDER}"} \
            ${PI_MODEL:+--model "${PI_MODEL}"} \
            &
        PI_BRIDGE_PID=$!
        echo "pi-bridge started with PID ${PI_BRIDGE_PID}"
    else
        if ! command -v pi-bridge &> /dev/null; then
            echo "Warning: pi-bridge enabled but pi-bridge binary not found"
        fi
        if ! command -v "${PI_EXECUTABLE}" &> /dev/null; then
            echo "Warning: pi-bridge enabled but Pi CLI (${PI_EXECUTABLE}) not found"
        fi
    fi
fi

# Give services a moment to start
sleep 1

# Start opencode serve in the foreground
echo "Starting opencode serve on port ${OPENCODE_PORT}..."
if ! command -v "${OPENCODE_BIN}" >/dev/null 2>&1; then
    if [ -x "${HOME_DIR}/.opencode/bin/opencode" ]; then
        OPENCODE_BIN="${HOME_DIR}/.opencode/bin/opencode"
    elif [ -x "${OPENCODE_SEED_DIR}/bin/opencode" ]; then
        OPENCODE_BIN="${OPENCODE_SEED_DIR}/bin/opencode"
    fi
fi
exec "${OPENCODE_BIN}" serve --port "${OPENCODE_PORT}" --hostname 0.0.0.0
