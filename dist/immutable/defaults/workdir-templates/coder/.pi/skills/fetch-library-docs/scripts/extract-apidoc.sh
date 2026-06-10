#!/bin/bash
# Extract APIDOC blocks from documentation
# APIDOC blocks contain structured API documentation with parameters, descriptions
# Usage: extract-apidoc.sh [max_blocks] < input

set -euo pipefail

MAX_BLOCKS="${1:-3}"

# Use awk to extract APIDOC blocks between ```APIDOC markers
awk -v max="$MAX_BLOCKS" '
  BEGIN {
    count = 0
    in_block = 0
    block = ""
  }

  /^```APIDOC/ {
    in_block = 1
    block = ""
    next
  }

  /^```$/ && in_block {
    if (count < max && length(block) > 50) {
      count++
      print "### API Reference " count
      print ""
      print block
      print ""
    }
    in_block = 0
    block = ""
    next
  }

  in_block {
    if (block != "") {
      block = block "\n" $0
    } else {
      block = $0
    }
  }

  END {
    # Also extract parameter documentation patterns outside APIDOC blocks
    # This is handled separately if no APIDOC blocks found
  }
'

# If no APIDOC blocks, try to extract parameter documentation
# Look for patterns like "### Parameters", "**param** (type)", etc.
