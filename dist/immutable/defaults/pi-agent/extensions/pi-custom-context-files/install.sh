#!/bin/bash
# Installation script for custom-context-files extension

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXTENSION_NAME="custom-context-files"

# Color output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}Installing ${EXTENSION_NAME} extension...${NC}"

# Check if user wants global or local installation
if [ "$1" = "--local" ] || [ "$1" = "-l" ]; then
    TARGET_DIR=".pi/extensions"
    echo -e "${YELLOW}Installing to project-local extensions (.pi/extensions/)${NC}"
else
    TARGET_DIR="$HOME/.pi/agent/extensions"
    echo -e "${YELLOW}Installing to global extensions (~/.pi/agent/extensions/)${NC}"
fi

# Create target directory if it doesn't exist
mkdir -p "$TARGET_DIR"

# Copy extension
echo "Copying extension to $TARGET_DIR/$EXTENSION_NAME"
cp -r "$SCRIPT_DIR" "$TARGET_DIR/"

# Create example config files
if [ "$1" != "--local" ] && [ "$1" != "-l" ]; then
    # Global example
    if [ ! -f "$HOME/.pi/agent/context.json" ]; then
        echo "Creating example global config: ~/.pi/agent/context.json"
        cp "$SCRIPT_DIR/examples/global-context.json" "$HOME/.pi/agent/context.json"
        echo -e "${GREEN}✓ Created ~/.pi/agent/context.json${NC}"
    else
        echo -e "${YELLOW}ℹ ~/.pi/agent/context.json already exists, skipping${NC}"
    fi
else
    # Project example
    if [ ! -f ".pi/context.json" ]; then
        echo "Creating example project config: .pi/context.json"
        mkdir -p .pi
        cp "$SCRIPT_DIR/examples/project-context.json" ".pi/context.json"
        echo -e "${GREEN}✓ Created .pi/context.json${NC}"
    else
        echo -e "${YELLOW}ℹ .pi/context.json already exists, skipping${NC}"
    fi
fi

echo ""
echo -e "${GREEN}✓ Installation complete!${NC}"
echo ""
echo "Next steps:"
echo "  1. Edit your context.json configuration files"
echo "  2. Create your custom context files (e.g., USERS.md, PERSONALITY.md)"
echo "  3. Restart pi or run /reload to load the extension"
echo ""
echo "See README.md for configuration details."
