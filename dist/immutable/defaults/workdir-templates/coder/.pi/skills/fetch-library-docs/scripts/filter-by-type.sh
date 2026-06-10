#!/bin/bash
# Filter content by type(s) - Main orchestrator
# Usage: filter-by-type.sh <content_types> <max_items> < raw_input
#
# content_types: comma-separated list (e.g., "examples,api-ref")
# max_items: max items per content type (default: 5)
#
# Supported types:
#   examples       - Code examples (js, typescript, python, etc.)
#   api-ref        - API documentation, signatures, APIDOC blocks
#   setup          - Installation, terminal commands
#   concepts       - Conceptual explanations (prose)
#   migration      - Before/after comparisons, breaking changes
#   troubleshooting - Workarounds, debugging tips
#   patterns       - Best practices, design patterns
#   all            - No filtering, return everything

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

CONTENT_TYPES="${1:-examples}"
MAX_ITEMS="${2:-5}"

# Read input once and store
RAW_INPUT=$(cat)

# If no input, exit early
if [ -z "$RAW_INPUT" ]; then
  echo "# No content to filter"
  exit 0
fi

# Handle 'all' type - no filtering
if [ "$CONTENT_TYPES" = "all" ]; then
  echo "$RAW_INPUT"
  exit 0
fi

OUTPUT=""

# Split content types by comma and process each
IFS=',' read -ra TYPES <<< "$CONTENT_TYPES"

for type in "${TYPES[@]}"; do
  # Trim whitespace
  type=$(echo "$type" | tr -d ' ')

  case "$type" in
    examples)
      RESULT=$(echo "$RAW_INPUT" | "$SCRIPT_DIR/extract-code-blocks.sh" "$MAX_ITEMS" 2>/dev/null || echo "")
      if [ -n "$RESULT" ] && [ "$RESULT" != "# No code blocks found" ]; then
        OUTPUT+="## Code Examples\n\n$RESULT\n\n"
      fi
      ;;
    api-ref)
      # Extract APIDOC blocks first
      APIDOC=$(echo "$RAW_INPUT" | "$SCRIPT_DIR/extract-apidoc.sh" "$MAX_ITEMS" 2>/dev/null || echo "")
      if [ -n "$APIDOC" ]; then
        OUTPUT+="## API Documentation\n\n$APIDOC\n\n"
      fi
      # Then extract signatures
      SIGS=$(echo "$RAW_INPUT" | "$SCRIPT_DIR/extract-signatures.sh" "$MAX_ITEMS" 2>/dev/null || echo "")
      if [ -n "$SIGS" ]; then
        OUTPUT+="## API Signatures\n\n$SIGS\n\n"
      fi
      ;;
    setup)
      RESULT=$(echo "$RAW_INPUT" | "$SCRIPT_DIR/extract-terminal-blocks.sh" "$MAX_ITEMS" 2>/dev/null || echo "")
      if [ -n "$RESULT" ] && [ "$RESULT" != "# No terminal blocks found" ]; then
        OUTPUT+="## Setup & Installation\n\n$RESULT\n\n"
      fi
      ;;
    concepts)
      RESULT=$(echo "$RAW_INPUT" | "$SCRIPT_DIR/extract-prose.sh" "$MAX_ITEMS" 2>/dev/null || echo "")
      if [ -n "$RESULT" ]; then
        OUTPUT+="## Concepts\n\n$RESULT\n\n"
      fi
      ;;
    migration)
      RESULT=$(echo "$RAW_INPUT" | "$SCRIPT_DIR/extract-migration.sh" "$MAX_ITEMS" 2>/dev/null || echo "")
      if [ -n "$RESULT" ]; then
        OUTPUT+="## Migration Guide\n\n$RESULT\n\n"
      fi
      ;;
    troubleshooting)
      RESULT=$(echo "$RAW_INPUT" | "$SCRIPT_DIR/extract-troubleshooting.sh" "$MAX_ITEMS" 2>/dev/null || echo "")
      if [ -n "$RESULT" ]; then
        OUTPUT+="## Troubleshooting\n\n$RESULT\n\n"
      fi
      ;;
    patterns)
      RESULT=$(echo "$RAW_INPUT" | "$SCRIPT_DIR/extract-patterns.sh" "$MAX_ITEMS" 2>/dev/null || echo "")
      if [ -n "$RESULT" ]; then
        OUTPUT+="## Best Practices\n\n$RESULT\n\n"
      fi
      ;;
    notes)
      RESULT=$(echo "$RAW_INPUT" | "$SCRIPT_DIR/extract-notes.sh" "$MAX_ITEMS" 2>/dev/null || echo "")
      if [ -n "$RESULT" ] && [ "$RESULT" != "- No important notes found" ]; then
        OUTPUT+="## Important Notes\n\n$RESULT\n\n"
      fi
      ;;
    *)
      echo "[WARNING] Unknown content type: $type" >&2
      ;;
  esac
done

# If no content extracted, return a message
if [ -z "$OUTPUT" ]; then
  echo "[CONTENT_TYPE_EMPTY]"
  echo ""
  echo "No content found for requested types: $CONTENT_TYPES"
  echo ""
  echo "Available types: examples, api-ref, setup, concepts, migration, troubleshooting, patterns, notes, all"
  exit 0
fi

# Output the filtered content
echo -e "$OUTPUT"
