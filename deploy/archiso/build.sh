#!/usr/bin/env bash
# Build Octo Arch Linux ISO
# Usage: ./build.sh [--with-binaries]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OCTO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BYTEOWLZ_ROOT="$(cd "$OCTO_ROOT/.." && pwd)"
WORK_DIR="/tmp/octo-archiso-work"
OUT_DIR="${OUT_DIR:-$HOME/octo-iso}"
BUILD_BINARIES=false

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[OK]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --with-binaries    Build and include all byteowlz binaries in the ISO"
    echo "  --work-dir DIR     Working directory (default: /tmp/octo-archiso-work)"
    echo "  --out-dir DIR      Output directory (default: ~/octo-iso)"
    echo "  -h, --help         Show this help"
    echo ""
    echo "Binaries included with --with-binaries:"
    echo "  Core:    octo, octoctl, octo-runner, octo-files"
    echo "  Tools:   mmry, trx, agntz, hstry, byt"
    echo "  Search:  sx, scrpr"
    echo "  LLM:     eavs, skdlr, tmpltr"
    echo "  Mail:    h8"
    echo "  Media:   sldr, kokorox, ears"
    echo "  Other:   dgrmr, cmfy, hmr, ignr, ingestr"
    echo ""
    echo "Requirements:"
    echo "  - archiso package installed"
    echo "  - Run as root (or with sudo)"
    echo ""
    echo "Example:"
    echo "  sudo ./build.sh --with-binaries"
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --with-binaries)
            BUILD_BINARIES=true
            shift
            ;;
        --work-dir)
            WORK_DIR="$2"
            shift 2
            ;;
        --out-dir)
            OUT_DIR="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Check requirements
if [[ $EUID -ne 0 ]]; then
    log_error "This script must be run as root"
    echo "Try: sudo $0 $*"
    exit 1
fi

if ! command -v mkarchiso &>/dev/null; then
    log_error "archiso is not installed"
    echo "Install with: pacman -S archiso"
    exit 1
fi

# Clean up previous build
log_info "Cleaning up previous build..."
rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR" "$OUT_DIR"

# Copy profile to work directory
log_info "Copying archiso profile..."
cp -r "$SCRIPT_DIR"/* "$WORK_DIR/"

# Setup Chaotic-AUR keyring
log_info "Setting up Chaotic-AUR repository..."
if ! pacman-key --list-keys 3056513887B78AEB &>/dev/null; then
    log_info "  Adding Chaotic-AUR key..."
    pacman-key --recv-key 3056513887B78AEB --keyserver keyserver.ubuntu.com
    pacman-key --lsign-key 3056513887B78AEB
fi

# Install chaotic-keyring and chaotic-mirrorlist if not present
if ! pacman -Qi chaotic-keyring &>/dev/null; then
    log_info "  Installing chaotic-keyring..."
    pacman -U --noconfirm 'https://cdn-mirror.chaotic.cx/chaotic-aur/chaotic-keyring.pkg.tar.zst'
fi
if ! pacman -Qi chaotic-mirrorlist &>/dev/null; then
    log_info "  Installing chaotic-mirrorlist..."
    pacman -U --noconfirm 'https://cdn-mirror.chaotic.cx/chaotic-aur/chaotic-mirrorlist.pkg.tar.zst'
fi

# Copy chaotic mirrorlist to work directory
cp /etc/pacman.d/chaotic-mirrorlist "$WORK_DIR/airootfs/etc/pacman.d/" 2>/dev/null || true

# Build binaries if requested
if [[ "$BUILD_BINARIES" == "true" ]]; then
    log_info "Building byteowlz binaries..."
    
    mkdir -p "$WORK_DIR/airootfs/usr/local/bin"
    
    # Function to build and copy a Rust binary
    build_rust_binary() {
        local name="$1"
        local dir="$2"
        local binary="${3:-$name}"
        
        if [[ -d "$dir" ]]; then
            log_info "  Building $name..."
            (cd "$dir" && cargo build --release 2>/dev/null) || {
                log_warn "  Failed to build $name, skipping"
                return 0
            }
            
            if [[ -f "$dir/target/release/$binary" ]]; then
                cp "$dir/target/release/$binary" "$WORK_DIR/airootfs/usr/local/bin/"
                log_success "  Copied $binary"
            else
                log_warn "  Binary $binary not found after build"
            fi
        else
            log_warn "  Directory $dir not found, skipping $name"
        fi
    }
    
    # Function to copy additional binaries from a workspace
    copy_workspace_binaries() {
        local dir="$1"
        shift
        local binaries=("$@")
        
        for binary in "${binaries[@]}"; do
            if [[ -f "$dir/target/release/$binary" ]]; then
                cp "$dir/target/release/$binary" "$WORK_DIR/airootfs/usr/local/bin/"
                log_success "  Copied $binary"
            fi
        done
    }
    
    echo ""
    log_info "=== Core Octo Components ==="
    
    # Octo backend (multiple binaries)
    if [[ -d "$OCTO_ROOT/backend" ]]; then
        log_info "  Building octo backend..."
        (cd "$OCTO_ROOT/backend" && cargo build --release 2>/dev/null) || log_warn "  Backend build failed"
        copy_workspace_binaries "$OCTO_ROOT" octo octoctl octo-runner pi-bridge octo-sandbox
    fi
    
    # Fileserver
    build_rust_binary "octo-files" "$OCTO_ROOT/fileserver" "octo-files"
    
    echo ""
    log_info "=== Agent Tools ==="
    
    # mmry - Memory system
    build_rust_binary "mmry" "$BYTEOWLZ_ROOT/mmry" "mmry"
    
    # trx - Task tracking
    build_rust_binary "trx" "$BYTEOWLZ_ROOT/trx" "trx"
    
    # agntz - Agent operations
    build_rust_binary "agntz" "$BYTEOWLZ_ROOT/agntz" "agntz"
    
    # hstry - History
    build_rust_binary "hstry" "$BYTEOWLZ_ROOT/hstry" "hstry"
    
    # byt - Cross-repo management
    build_rust_binary "byt" "$BYTEOWLZ_ROOT/byt" "byt"
    
    echo ""
    log_info "=== Search Tools ==="
    
    # sx - SearXNG search
    build_rust_binary "sx" "$BYTEOWLZ_ROOT/sx" "sx"
    
    # scrpr - Scraper
    build_rust_binary "scrpr" "$BYTEOWLZ_ROOT/scrpr" "scrpr"
    
    echo ""
    log_info "=== LLM Tools ==="
    
    # eavs - LLM proxy
    build_rust_binary "eavs" "$BYTEOWLZ_ROOT/eavs" "eavs"
    
    # skdlr - Scheduler
    build_rust_binary "skdlr" "$BYTEOWLZ_ROOT/skdlr" "skdlr"
    
    # tmpltr - Templater
    build_rust_binary "tmpltr" "$BYTEOWLZ_ROOT/tmpltr" "tmpltr"
    
    echo ""
    log_info "=== Communication Tools ==="
    
    # h8 - Exchange/mail client
    build_rust_binary "h8" "$BYTEOWLZ_ROOT/h8" "h8"
    
    # mailz - Agent messaging
    build_rust_binary "mailz" "$BYTEOWLZ_ROOT/mailz" "mailz"
    
    echo ""
    log_info "=== Media Tools ==="
    
    # sldr - Slider/media
    build_rust_binary "sldr" "$BYTEOWLZ_ROOT/sldr" "sldr"
    
    # kokorox - TTS
    build_rust_binary "kokorox" "$BYTEOWLZ_ROOT/kokorox" "kokorox"
    
    # eaRS - STT  
    build_rust_binary "ears" "$BYTEOWLZ_ROOT/eaRS" "ears"
    
    echo ""
    log_info "=== Other Tools ==="
    
    # dgrmr - Diagrammer
    build_rust_binary "dgrmr" "$BYTEOWLZ_ROOT/dgrmr" "dgrmr"
    
    # cmfy - ComfyUI client
    build_rust_binary "cmfy" "$BYTEOWLZ_ROOT/cmfy" "cmfy"
    
    # hmr - Hammer
    build_rust_binary "hmr" "$BYTEOWLZ_ROOT/hmr" "hmr"
    
    # ignr - Ignore
    build_rust_binary "ignr" "$BYTEOWLZ_ROOT/ignr" "ignr"
    
    # ingestr - Ingester
    build_rust_binary "ingestr" "$BYTEOWLZ_ROOT/ingestr" "ingestr"
    
    echo ""
    
    # Make all binaries executable and world-readable
    chmod 755 "$WORK_DIR/airootfs/usr/local/bin/"* 2>/dev/null || true
    
    # List what was copied
    log_info "Binaries included in ISO:"
    ls -1 "$WORK_DIR/airootfs/usr/local/bin/" 2>/dev/null | sed 's/^/  /'
    echo ""
fi

# Build the ISO
log_info "Building ISO (this may take several minutes)..."
cd "$WORK_DIR"
mkarchiso -v -w "$WORK_DIR/work" -o "$OUT_DIR" "$WORK_DIR"

# Show result
ISO_FILE=$(ls -t "$OUT_DIR"/*.iso 2>/dev/null | head -1)
if [[ -n "$ISO_FILE" ]]; then
    echo ""
    log_success "ISO built successfully!"
    echo ""
    echo "Output: $ISO_FILE"
    echo "Size:   $(du -h "$ISO_FILE" | cut -f1)"
    echo ""
    echo "To write to USB:"
    echo "  sudo dd if=$ISO_FILE of=/dev/sdX bs=4M status=progress oflag=sync"
    echo ""
    echo "After booting, run:"
    echo "  octo-install"
else
    log_error "ISO build failed"
    exit 1
fi
