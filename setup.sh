#!/usr/bin/env bash
#
# Oqto Setup Script
# Comprehensive setup and onboarding for the Oqto AI Agent Workspace Platform
#
# Supports:
#   - Linux (primary target, fully supported)
#   - macOS (experimental, single-user only, some services need manual setup)
#   - Single-user and multi-user modes
#   - Local (native processes) and container (Docker/Podman) modes
#
# Usage:
#   ./setup.sh                  # Interactive mode
#   ./setup.sh --non-interactive # Use defaults or environment variables
#   ./setup.sh --help           # Show help
#
# Modules are loaded from scripts/setup/ in numbered order.
# Each module is self-contained and can be edited independently.

set -uo pipefail

# Resolve the directory containing this script (follows symlinks)
SETUP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SETUP_MODULES_DIR="${SETUP_DIR}/scripts/setup"

if [[ ! -d "$SETUP_MODULES_DIR" ]]; then
  echo "ERROR: Setup modules directory not found: $SETUP_MODULES_DIR" >&2
  echo "Expected setup.sh to be at the root of the oqto repo." >&2
  exit 1
fi

# Source all modules in order
for module in "$SETUP_MODULES_DIR"/[0-9]*.sh; do
  if [[ -f "$module" ]]; then
    # shellcheck source=/dev/null
    source "$module"
  fi
done

# Run main
main "$@"
