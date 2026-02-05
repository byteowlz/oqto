#!/usr/bin/env bash
# Development script to spin up a podman container with opencode serve,
# file server, ttyd terminal, and the Next.js frontend.
#
# Usage:
#   ./dev.sh [workspace_path]
#
# Arguments:
#   workspace_path  Path to mount as workspace (default: current directory)
#
# Environment:
#   CONTAINER_NAME   Name for the container (default: octo-dev)
#   OPENCODE_PORT    Port for opencode serve (default: 41820)
#   FILESERVER_PORT  Port for file server (default: 41821)
#   TTYD_PORT        Port for ttyd terminal (default: 41822)
#   FRONTEND_PORT    Port for Next.js frontend (default: 3000)
#   BUILD_IMAGE      Set to 1 to force rebuild the image (default: 0)

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTAINER_DIR="$(dirname "$SCRIPT_DIR")"
FRONTEND_DIR="$(dirname "$CONTAINER_DIR")/frontend"

# Configuration with defaults
CONTAINER_NAME="${CONTAINER_NAME:-octo-dev}"
OPENCODE_PORT="${OPENCODE_PORT:-41820}"
FILESERVER_PORT="${FILESERVER_PORT:-41821}"
TTYD_PORT="${TTYD_PORT:-41822}"
FRONTEND_PORT="${FRONTEND_PORT:-3000}"
BUILD_IMAGE="${BUILD_IMAGE:-0}"
WORKSPACE_PATH="${1:-$(pwd)}"

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)
        IMAGE_TAG="octo-dev:x86_64"
        DOCKERFILE="Dockerfile"
        ;;
    arm64|aarch64)
        IMAGE_TAG="octo-dev:arm64"
        DOCKERFILE="Dockerfile.arm64"
        ;;
    *)
        echo "Error: Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Cleanup function
cleanup() {
    log_info "Shutting down..."
    
    # Stop frontend if running
    if [[ -n "${FRONTEND_PID:-}" ]]; then
        log_info "Stopping frontend (PID: $FRONTEND_PID)"
        kill "$FRONTEND_PID" 2>/dev/null || true
        wait "$FRONTEND_PID" 2>/dev/null || true
    fi
    
    # Stop container
    if podman ps --format "{{.Names}}" | grep -q "^${CONTAINER_NAME}$"; then
        log_info "Stopping container: $CONTAINER_NAME"
        podman stop "$CONTAINER_NAME" 2>/dev/null || true
    fi
    
    log_success "Cleanup complete"
    exit 0
}

trap cleanup SIGINT SIGTERM EXIT

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check for podman
    if ! command -v podman &>/dev/null; then
        log_error "podman is not installed. Please install podman first."
        exit 1
    fi
    
    # Check for bun (for frontend)
    if ! command -v bun &>/dev/null; then
        log_error "bun is not installed. Please install bun first."
        exit 1
    fi
    
    # Check workspace path exists
    if [[ ! -d "$WORKSPACE_PATH" ]]; then
        log_error "Workspace path does not exist: $WORKSPACE_PATH"
        exit 1
    fi
    
    # Convert to absolute path
    WORKSPACE_PATH="$(cd "$WORKSPACE_PATH" && pwd)"
    
    log_success "Prerequisites OK"
}

# Build or pull the container image
ensure_image() {
    log_info "Checking for container image: $IMAGE_TAG"
    
    if [[ "$BUILD_IMAGE" == "1" ]] || ! podman image exists "$IMAGE_TAG"; then
        log_info "Building container image from $DOCKERFILE..."
        podman build \
            -t "$IMAGE_TAG" \
            -f "$CONTAINER_DIR/$DOCKERFILE" \
            --build-arg OPENCODE_PORT="$OPENCODE_PORT" \
            --build-arg FILESERVER_PORT="$FILESERVER_PORT" \
            --build-arg TTYD_PORT="$TTYD_PORT" \
            "$CONTAINER_DIR"
        log_success "Image built: $IMAGE_TAG"
    else
        log_success "Image exists: $IMAGE_TAG"
    fi
}

# Stop existing container if running
stop_existing_container() {
    if podman ps --format "{{.Names}}" | grep -q "^${CONTAINER_NAME}$"; then
        log_warn "Container $CONTAINER_NAME is already running. Stopping..."
        podman stop "$CONTAINER_NAME" 2>/dev/null || true
        podman rm "$CONTAINER_NAME" 2>/dev/null || true
    elif podman ps -a --format "{{.Names}}" | grep -q "^${CONTAINER_NAME}$"; then
        log_info "Removing stopped container: $CONTAINER_NAME"
        podman rm "$CONTAINER_NAME" 2>/dev/null || true
    fi
}

# Start the container
start_container() {
    log_info "Starting container: $CONTAINER_NAME"
    log_info "  Workspace: $WORKSPACE_PATH"
    log_info "  OpenCode port: $OPENCODE_PORT"
    log_info "  File server port: $FILESERVER_PORT"
    log_info "  TTYd terminal port: $TTYD_PORT"
    
    # Load environment variables from .env if it exists
    ENV_ARGS=()
    if [[ -f "$CONTAINER_DIR/.env" ]]; then
        log_info "Loading environment from .env file"
        while IFS='=' read -r key value; do
            # Skip comments and empty lines
            [[ -z "$key" || "$key" =~ ^# ]] && continue
            # Remove quotes from value
            value="${value%\"}"
            value="${value#\"}"
            ENV_ARGS+=("-e" "$key=$value")
        done < "$CONTAINER_DIR/.env"
    fi
    
    podman run -d \
        --name "$CONTAINER_NAME" \
        --hostname "$CONTAINER_NAME" \
        -p "${OPENCODE_PORT}:${OPENCODE_PORT}" \
        -p "${FILESERVER_PORT}:${FILESERVER_PORT}" \
        -p "${TTYD_PORT}:${TTYD_PORT}" \
        -v "${WORKSPACE_PATH}:/home/dev/workspace:Z" \
        -e "OPENCODE_PORT=${OPENCODE_PORT}" \
        -e "FILESERVER_PORT=${FILESERVER_PORT}" \
        -e "TTYD_PORT=${TTYD_PORT}" \
        "${ENV_ARGS[@]}" \
        "$IMAGE_TAG"
    
    log_success "Container started: $CONTAINER_NAME"
    
    # Wait for services to be ready
    log_info "Waiting for services to start..."
    sleep 3
    
    # Check if container is still running
    if ! podman ps --format "{{.Names}}" | grep -q "^${CONTAINER_NAME}$"; then
        log_error "Container failed to start. Logs:"
        podman logs "$CONTAINER_NAME"
        exit 1
    fi
    
    # Check if opencode is responding
    local max_attempts=30
    local attempt=0
    while [[ $attempt -lt $max_attempts ]]; do
        if curl -s "http://localhost:${OPENCODE_PORT}/session" &>/dev/null; then
            log_success "OpenCode server is ready"
            break
        fi
        attempt=$((attempt + 1))
        sleep 1
    done
    
    if [[ $attempt -eq $max_attempts ]]; then
        log_warn "OpenCode server may not be ready yet. Check logs with: podman logs $CONTAINER_NAME"
    fi
}

# Install frontend dependencies if needed
install_frontend_deps() {
    if [[ ! -d "$FRONTEND_DIR/node_modules" ]]; then
        log_info "Installing frontend dependencies..."
        (cd "$FRONTEND_DIR" && bun install)
        log_success "Frontend dependencies installed"
    fi
}

# Start the frontend
start_frontend() {
    log_info "Starting frontend on port $FRONTEND_PORT..."
    
    # Export environment variables for frontend
    export NEXT_PUBLIC_OPENCODE_BASE_URL="http://localhost:${OPENCODE_PORT}"
    export NEXT_PUBLIC_FILE_SERVER_URL="http://localhost:${FILESERVER_PORT}"
    export NEXT_PUBLIC_TERMINAL_WS_URL="ws://localhost:${TTYD_PORT}"
    # Clear Caddy URL for direct connection mode
    export NEXT_PUBLIC_CADDY_BASE_URL=""
    
    (cd "$FRONTEND_DIR" && bun dev --port "$FRONTEND_PORT") &
    FRONTEND_PID=$!
    
    log_success "Frontend starting (PID: $FRONTEND_PID)"
}

# Print status and URLs
print_status() {
    echo ""
    echo "======================================================"
    echo -e "${GREEN}Octo Development Environment${NC}"
    echo "======================================================"
    echo ""
    echo "Services:"
    echo "  - Frontend:     http://localhost:${FRONTEND_PORT}"
    echo "  - OpenCode API: http://localhost:${OPENCODE_PORT}"
    echo "  - File Server:  http://localhost:${FILESERVER_PORT}"
    echo "  - Web Terminal: http://localhost:${TTYD_PORT}"
    echo ""
    echo "Workspace: $WORKSPACE_PATH"
    echo ""
    echo "Container: $CONTAINER_NAME"
    echo "  View logs: podman logs -f $CONTAINER_NAME"
    echo "  Shell:     podman exec -it $CONTAINER_NAME bash"
    echo ""
    echo "Press Ctrl+C to stop all services"
    echo "======================================================"
    echo ""
}

# Main
main() {
    echo ""
    log_info "Octo Development Environment"
    echo ""
    
    check_prerequisites
    ensure_image
    stop_existing_container
    start_container
    install_frontend_deps
    start_frontend
    print_status
    
    # Wait for frontend process
    wait "$FRONTEND_PID"
}

main "$@"
