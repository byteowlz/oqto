#!/usr/bin/env bash
#
# Start fileserver alongside opencode serve
# 
# Usage: start-with-opencode.sh [OPTIONS]
#
# The fileserver will run on OPENCODE_PORT + 1 by default.
# Both processes are managed together - killing this script kills both.

set -euo pipefail

# Default values
OPENCODE_PORT="${OPENCODE_PORT:-4096}"
FILESERVER_PORT="${FILESERVER_PORT:-$((OPENCODE_PORT + 1))}"
FILESERVER_ROOT="${FILESERVER_ROOT:-.}"
FILESERVER_BIND="${FILESERVER_BIND:-0.0.0.0}"
VERBOSE="${VERBOSE:-false}"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --opencode-port)
            OPENCODE_PORT="$2"
            FILESERVER_PORT=$((OPENCODE_PORT + 1))
            shift 2
            ;;
        --fileserver-port)
            FILESERVER_PORT="$2"
            shift 2
            ;;
        --root)
            FILESERVER_ROOT="$2"
            shift 2
            ;;
        --bind)
            FILESERVER_BIND="$2"
            shift 2
            ;;
        --verbose|-v)
            VERBOSE="true"
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Start fileserver alongside opencode serve"
            echo ""
            echo "Options:"
            echo "  --opencode-port PORT    Port for opencode serve (default: 4096)"
            echo "  --fileserver-port PORT  Port for fileserver (default: opencode-port + 1)"
            echo "  --root DIR              Root directory to serve (default: .)"
            echo "  --bind ADDR             Address to bind to (default: 0.0.0.0)"
            echo "  --verbose, -v           Enable verbose logging"
            echo "  --help, -h              Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "Starting services..."
echo "  Opencode port: ${OPENCODE_PORT}"
echo "  Fileserver port: ${FILESERVER_PORT}"
echo "  Fileserver root: ${FILESERVER_ROOT}"

# Trap to cleanup both processes on exit
cleanup() {
    echo "Shutting down..."
    kill $OPENCODE_PID 2>/dev/null || true
    kill $FILESERVER_PID 2>/dev/null || true
    wait
}
trap cleanup EXIT INT TERM

# Build fileserver args
FILESERVER_ARGS=("--port" "${FILESERVER_PORT}" "--bind" "${FILESERVER_BIND}" "--root" "${FILESERVER_ROOT}")
if [[ "${VERBOSE}" == "true" ]]; then
    FILESERVER_ARGS+=("--verbose")
fi

# Start opencode serve in background
echo "Starting opencode serve on port ${OPENCODE_PORT}..."
opencode serve --port "${OPENCODE_PORT}" &
OPENCODE_PID=$!

# Give opencode a moment to start
sleep 1

# Start fileserver in background
echo "Starting fileserver on port ${FILESERVER_PORT}..."
fileserver "${FILESERVER_ARGS[@]}" &
FILESERVER_PID=$!

echo "Both services started. Press Ctrl+C to stop."

# Wait for either process to exit
wait -n $OPENCODE_PID $FILESERVER_PID
EXIT_CODE=$?

echo "A service exited with code ${EXIT_CODE}"
exit $EXIT_CODE
