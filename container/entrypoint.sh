#!/usr/bin/env bash
set -e

# Default ports
OPENCODE_PORT="${OPENCODE_PORT:-41820}"
FILESERVER_PORT="${FILESERVER_PORT:-41821}"
TTYD_PORT="${TTYD_PORT:-41822}"
WORKSPACE_DIR="${WORKSPACE_DIR:-/home/dev/workspace}"

echo "Starting OpenCode development container..."
echo "OpenCode server port: ${OPENCODE_PORT}"
echo "File server port: ${FILESERVER_PORT}"
echo "TTY terminal port: ${TTYD_PORT}"
echo "Workspace directory: ${WORKSPACE_DIR}"

# Ensure workspace directory exists
mkdir -p "${WORKSPACE_DIR}"
cd "${WORKSPACE_DIR}"

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
    python -m http.server "${FILESERVER_PORT}" --bind 0.0.0.0 --directory "${WORKSPACE_DIR}" &
fi
FILE_SERVER_PID=$!

# Start ttyd web terminal in the background
echo "Starting ttyd terminal on port ${TTYD_PORT}..."
ttyd \
    --port "${TTYD_PORT}" \
    --interface 0.0.0.0 \
    --writable \
    --cwd "${WORKSPACE_DIR}" \
    bash -l &
TTYD_PID=$!

# Give services a moment to start
sleep 1

# Start opencode serve in the foreground
echo "Starting opencode serve on port ${OPENCODE_PORT}..."
exec /home/dev/.opencode/bin/opencode serve --port "${OPENCODE_PORT}" --hostname 0.0.0.0
