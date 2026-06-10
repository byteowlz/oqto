#!/bin/bash
# Extract terminal/bash code blocks for setup and installation
# Targets: ```bash, ```terminal, ```shell, ```sh, ```zsh
# Usage: extract-terminal-blocks.sh [max_blocks] < input

set -euo pipefail

MAX_BLOCKS="${1:-5}"

# Use awk to extract terminal blocks
awk -v max="$MAX_BLOCKS" '
  BEGIN {
    count = 0
    in_block = 0
    block = ""
    lang = ""
  }

  /^```(bash|terminal|shell|sh|zsh|console)/ {
    in_block = 1
    block = ""
    lang = substr($0, 4)
    next
  }

  /^```$/ && in_block {
    if (count < max && length(block) > 10) {
      count++
      print "### Command " count
      print "```bash"
      print block
      print "```"
      print ""
    }
    in_block = 0
    block = ""
    lang = ""
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
    if (count == 0) {
      print "# No terminal blocks found"
    }
  }
'
