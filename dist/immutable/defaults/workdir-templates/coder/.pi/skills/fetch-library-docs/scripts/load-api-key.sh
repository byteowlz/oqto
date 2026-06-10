#!/bin/bash
# Load Context7 API key from multiple sources (hybrid approach)
#
# Priority order:
# 1. Environment variable (CONTEXT7_API_KEY) - for advanced users/CI
# 2. Project config (.context7.env in current dir) - for project-specific keys
# 3. User config (~/.context7.env) - for personal default
# 4. Empty (returns error message for Claude to handle)
#
# Usage: source load-api-key.sh
#        PYTHON_CMD=$(get_python_cmd)
#        API_KEY=$(load_context7_api_key)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source Python detection
source "$SCRIPT_DIR/detect-python.sh"

# Get Python command (cached)
get_python_cmd() {
  if [ -z "${_PYTHON_CMD:-}" ]; then
    _PYTHON_CMD=$(detect_python)
  fi
  echo "$_PYTHON_CMD"
}

load_context7_api_key() {
  # 1. Check environment variable first (highest priority)
  if [ -n "${CONTEXT7_API_KEY:-}" ]; then
    echo "$CONTEXT7_API_KEY"
    return 0
  fi

  # 2. Check project-level config file
  if [ -f ".context7.env" ]; then
    local key=$(grep -E '^CONTEXT7_API_KEY=' ".context7.env" 2>/dev/null | cut -d'=' -f2- | tr -d '"' | tr -d "'")
    if [ -n "$key" ]; then
      echo "$key"
      return 0
    fi
  fi

  # 3. Check user-level config file
  if [ -f "$HOME/.context7.env" ]; then
    local key=$(grep -E '^CONTEXT7_API_KEY=' "$HOME/.context7.env" 2>/dev/null | cut -d'=' -f2- | tr -d '"' | tr -d "'")
    if [ -n "$key" ]; then
      echo "$key"
      return 0
    fi
  fi

  # 4. No key found
  return 1
}

# Build MCP command with or without API key
build_mcp_command() {
  local api_key=$(load_context7_api_key)

  if [ -n "$api_key" ]; then
    echo "npx -y @upstash/context7-mcp --api-key $api_key"
  else
    echo "npx -y @upstash/context7-mcp"
  fi
}

# Check if API key is configured (for status checks)
has_api_key() {
  local key=$(load_context7_api_key)
  [ -n "$key" ]
}

# Get API key source for debugging
get_api_key_source() {
  if [ -n "${CONTEXT7_API_KEY:-}" ]; then
    echo "environment variable"
  elif [ -f ".context7.env" ] && grep -qE '^CONTEXT7_API_KEY=' ".context7.env" 2>/dev/null; then
    echo "project config (.context7.env)"
  elif [ -f "$HOME/.context7.env" ] && grep -qE '^CONTEXT7_API_KEY=' "$HOME/.context7.env" 2>/dev/null; then
    echo "user config (~/.context7.env)"
  else
    echo "none"
  fi
}

# Generate structured error message for missing API key (for Claude to parse)
api_key_error_message() {
  cat << 'EOF'
[CONTEXT7_API_KEY_MISSING]

Context7 API key is not configured. The skill cannot fetch documentation without it.

## How to Fix

Ask the user to provide their Context7 API key, then save it using one of these methods:

**Option 1:** Save to config file
```bash
echo "CONTEXT7_API_KEY=<user_provided_key>" > ~/.context7.env
```

**Option 2:** Set environment variable (temporary)
```bash
export CONTEXT7_API_KEY=<user_provided_key>
```

## Get a Free API Key

Direct the user to: https://context7.com/dashboard

The API key starts with `ctx7sk_` or `ctx7sk-`
EOF
}
