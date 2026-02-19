#!/usr/bin/env bash
#
# Convert oqto.setup.toml (from oqto-website) to vm.tests.toml format
# This allows using the web configurator to define test scenarios
#
# Usage:
#   ./convert-setup-toml.sh /path/to/oqto.setup.toml
#   ./convert-setup-toml.sh /path/to/oqto.setup.toml >> vm.tests.toml
#

set -euo pipefail

SETUP_FILE="${1:-}"

if [[ -z "$SETUP_FILE" ]]; then
    echo "Usage: $0 <path-to-oqto.setup.toml>" >&2
    echo "" >&2
    echo "Converts oqto.setup.toml from the website configurator" >&2
    echo "to a vm.tests.toml scenario block." >&2
    exit 1
fi

if [[ ! -f "$SETUP_FILE" ]]; then
    echo "Error: File not found: $SETUP_FILE" >&2
    exit 1
fi

# Parse values from setup.toml
parse_val() {
    local key="$1"
    grep -E "^${key}\s*=" "$SETUP_FILE" 2>/dev/null | \
        sed -E 's/^[^=]+=\s*//' | \
        sed -E 's/^"(.*)"$/\1/' | \
        sed -E "s/^'(.*)'$/\1/" | \
        head -1
}

# Generate scenario name from config
backend=$(parse_val "backend_mode")
user_mode=$(parse_val "user_mode")
distro="ubuntu-24.04"  # Default, can be overridden

scenario_name="${distro}-${backend}-${user_mode}"

# Output scenario block
cat << EOF

[[scenario]]
name = "${scenario_name}"
description = "$(parse_val "distro" || echo "$distro") with ${backend} backend, ${user_mode} mode (from oqto.setup.toml)"
distro = "${distro}"
backend_mode = "${backend}"
user_mode = "${user_mode}"
container_runtime = "$(parse_val "container_runtime" || echo "")"
production = $(if [[ "$(parse_val "dev_mode")" == "true" ]]; then echo "false"; else echo "true"; fi)
EOF

# Check for providers
if grep -q "^\[providers" "$SETUP_FILE" 2>/dev/null; then
    echo ""
    echo "# NOTE: Providers defined in oqto.setup.toml - ensure they are in your vm.tests.toml [providers] section"
fi

echo ""
echo "# Copy this scenario block into your vm.tests.toml file" >&2