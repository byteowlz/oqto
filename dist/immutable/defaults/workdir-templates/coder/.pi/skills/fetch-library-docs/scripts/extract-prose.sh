#!/bin/bash
# Extract conceptual prose (paragraphs without code)
# Filters out code blocks and extracts meaningful explanatory text
# Usage: extract-prose.sh [max_paragraphs] < input

set -euo pipefail

MAX_PARAGRAPHS="${1:-5}"

# Use awk to extract prose paragraphs, skipping code blocks
awk -v max="$MAX_PARAGRAPHS" '
  BEGIN {
    count = 0
    in_code = 0
    paragraph = ""
    in_paragraph = 0
  }

  # Skip code blocks
  /^```/ {
    in_code = !in_code
    next
  }

  in_code { next }

  # Skip horizontal rules
  /^---+$/ || /^===+$/ { next }

  # Skip source/link lines
  /^Source:/ { next }

  # Skip empty lines - they end paragraphs
  /^[[:space:]]*$/ {
    if (in_paragraph && length(paragraph) > 100 && count < max) {
      # Only keep substantial paragraphs (>100 chars)
      count++
      print paragraph
      print ""
    }
    paragraph = ""
    in_paragraph = 0
    next
  }

  # Skip headers (they are kept as context markers)
  /^###/ {
    if (count < max) {
      print $0
      print ""
    }
    next
  }

  # Skip list items and short lines (likely not conceptual prose)
  /^[*-] / { next }
  /^\*\*[a-zA-Z]/ { next }

  # Accumulate paragraph text
  {
    if (in_paragraph) {
      paragraph = paragraph " " $0
    } else {
      paragraph = $0
      in_paragraph = 1
    }
  }

  END {
    # Output last paragraph if substantial
    if (in_paragraph && length(paragraph) > 100 && count < max) {
      print paragraph
      print ""
    }
  }
'
