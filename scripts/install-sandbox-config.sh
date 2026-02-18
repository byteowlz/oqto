#!/usr/bin/env bash
#
# Install Sandbox Configuration for Oqto Multi-User Mode
#
# This script copies the sandbox.toml from the admin user's config directory
# to /etc/oqto/sandbox.toml and sets up proper permissions so that:
# - The file is owned by root (cannot be modified by oqto users)
# - The oqto group can read it (for oqto-runner to load)
# - octo_* users cannot modify their own sandbox restrictions
#
# Usage:
#   sudo ./scripts/install-sandbox-config.sh
#   sudo ./scripts/install-sandbox-config.sh /path/to/custom/sandbox.toml
#
# The script must be run as root or with sudo.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[OK]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# Check if running as root
if [[ $EUID -ne 0 ]]; then
    error "This script must be run as root (use sudo)"
    exit 1
fi

# Determine source file
if [[ $# -ge 1 ]]; then
    SOURCE_FILE="$1"
else
    # Try to find the admin user's sandbox.toml
    # Look for the user who invoked sudo, or the owner of this script
    if [[ -n "${SUDO_USER:-}" ]]; then
        ADMIN_USER="$SUDO_USER"
    else
        # Fallback: find the first non-root user in the oqto group
        ADMIN_USER=$(getent group oqto 2>/dev/null | cut -d: -f4 | tr ',' '\n' | grep -v '^octo_' | head -1 || true)
    fi

    if [[ -z "$ADMIN_USER" ]]; then
        error "Could not determine admin user. Please specify the source file:"
        error "  sudo $0 /path/to/sandbox.toml"
        exit 1
    fi

    ADMIN_HOME=$(getent passwd "$ADMIN_USER" | cut -d: -f6)
    SOURCE_FILE="${ADMIN_HOME}/.config/oqto/sandbox.toml"
fi

# Validate source file
if [[ ! -f "$SOURCE_FILE" ]]; then
    error "Source file not found: $SOURCE_FILE"
    echo ""
    echo "Create a sandbox.toml first:"
    echo ""
    echo "  # Copy the example config"
    echo "  mkdir -p ~/.config/oqto"
    echo "  cp backend/crates/oqto/examples/sandbox.toml ~/.config/oqto/"
    echo ""
    echo "  # Then run this script again"
    echo "  sudo $0"
    echo ""
    echo "Or create a minimal config:"
    echo ""
    cat <<'EOF'
# ~/.config/oqto/sandbox.toml
enabled = true
profile = "development"

[profiles.development]
deny_read = ["~/.ssh", "~/.gnupg", "~/.aws"]
allow_write = ["~/.cargo", "~/.npm", "~/.bun", "/tmp"]
isolate_pid = true
EOF
    exit 1
fi

info "Source file: $SOURCE_FILE"

# Target locations
TARGET_DIR="/etc/oqto"
TARGET_FILE="${TARGET_DIR}/sandbox.toml"

# Check if oqto group exists
if ! getent group oqto >/dev/null 2>&1; then
    error "The 'oqto' group does not exist."
    echo "Create it with: sudo groupadd oqto"
    exit 1
fi

# Create target directory
if [[ ! -d "$TARGET_DIR" ]]; then
    info "Creating directory: $TARGET_DIR"
    mkdir -p "$TARGET_DIR"
    chown root:oqto "$TARGET_DIR"
    chmod 750 "$TARGET_DIR"
    success "Created $TARGET_DIR"
fi

# Backup existing file if it exists
if [[ -f "$TARGET_FILE" ]]; then
    BACKUP="${TARGET_FILE}.backup.$(date +%Y%m%d_%H%M%S)"
    info "Backing up existing config to: $BACKUP"
    cp "$TARGET_FILE" "$BACKUP"
    chown root:root "$BACKUP"
    chmod 600 "$BACKUP"
fi

# Copy the file
info "Copying sandbox config..."
cp "$SOURCE_FILE" "$TARGET_FILE"

# Set ownership: root owns it (cannot be modified by oqto users)
chown root:oqto "$TARGET_FILE"

# Set permissions:
# - Owner (root): read/write (640)
# - Group (oqto): read only
# - Others: no access
chmod 640 "$TARGET_FILE"

success "Installed sandbox config to $TARGET_FILE"

# Verify the installation
echo ""
info "Verifying installation..."
echo "  File: $TARGET_FILE"
echo "  Owner: $(stat -c '%U:%G' "$TARGET_FILE")"
echo "  Permissions: $(stat -c '%a' "$TARGET_FILE") ($(stat -c '%A' "$TARGET_FILE"))"

# Check if bwrap is installed
echo ""
if command -v bwrap >/dev/null 2>&1; then
    success "bubblewrap (bwrap) is installed: $(command -v bwrap)"
else
    warn "bubblewrap (bwrap) is NOT installed!"
    echo "  Install it with:"
    echo "    Debian/Ubuntu: sudo apt install bubblewrap"
    echo "    Fedora/RHEL:   sudo dnf install bubblewrap"
    echo "    Arch:          sudo pacman -S bubblewrap"
fi

# Show next steps
echo ""
info "Next steps:"
echo "  1. Restart oqto-runner to pick up the new config:"
echo "     systemctl --user restart oqto-runner"
echo ""
echo "  2. Or for system-wide oqto-runner:"
echo "     sudo systemctl restart oqto-runner"
echo ""
echo "  3. Verify sandbox is active by checking logs for:"
echo "     'Sandbox enabled - processes will be wrapped with bwrap'"

# Optional: Validate TOML syntax
if command -v toml >/dev/null 2>&1 || command -v taplo >/dev/null 2>&1; then
    echo ""
    info "Validating TOML syntax..."
    if command -v taplo >/dev/null 2>&1; then
        if taplo check "$TARGET_FILE" 2>/dev/null; then
            success "TOML syntax is valid"
        else
            warn "TOML validation failed - check the file for syntax errors"
        fi
    fi
fi

echo ""
success "Sandbox configuration installed successfully!"
