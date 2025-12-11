#!/usr/bin/env bash
set -e

# Default ports
OPENCODE_PORT="${OPENCODE_PORT:-8080}"
FILESERVER_PORT="${FILESERVER_PORT:-8081}"
WORKSPACE_DIR="${WORKSPACE_DIR:-/home/dev/workspace}"

echo "Starting OpenCode development container..."
echo "OpenCode server port: ${OPENCODE_PORT}"
echo "File server port: ${FILESERVER_PORT}"
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
python -m http.server "${FILESERVER_PORT}" --bind 0.0.0.0 --directory "${WORKSPACE_DIR}" &
FILE_SERVER_PID=$!

# Give file server a moment to start
sleep 1

# Start opencode serve in the foreground
echo "Starting opencode serve on port ${OPENCODE_PORT}..."
exec opencode serve --port "${OPENCODE_PORT}" --host 0.0.0.0
