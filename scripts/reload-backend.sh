#!/usr/bin/env bash
# Reload backend: rebuild, reinstall, and restart oqto serve
# Usage: ./scripts/reload-backend.sh [--no-start]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OQTO_DIR="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Find and kill existing oqto processes
kill_octo() {
    log_info "Stopping existing oqto processes..."
    pkill -f "oqto serve" 2>/dev/null || true
    pkill -f "oqto-backend" 2>/dev/null || true
    sleep 1
}

# Start oqto serve
start_octo() {
    log_info "Starting oqto serve --local-mode..."
    cd "$HOME"
    oqto serve --local-mode &
    OQTO_PID=$!
    log_success "oqto serve started (PID: $OQTO_PID)"
}

# One-shot reload
reload_once() {
    local no_start="${1:-false}"
    
    log_info "Building and installing..."
    cd "$OQTO_DIR"
    just install
    
    kill_octo
    
    if [ "$no_start" = "true" ]; then
        log_success "Build complete! Server stopped."
    else
        start_octo
        log_success "Reload complete!"
    fi
}

# Main
NO_START=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-start|-n)
            NO_START=true
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --no-start, -n  Build and stop server, but don't restart"
            echo "  --help, -h      Show this help message"
            echo ""
            echo "Without options: build, stop, and restart server."
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

reload_once "$NO_START"
