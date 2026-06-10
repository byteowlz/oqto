#!/bin/bash
# Extract migration-related content (before/after, breaking changes, upgrades)
# Targets: // Before, // After, Breaking change, migration, upgrade patterns
# Usage: extract-migration.sh [max_sections] < input

set -euo pipefail

MAX_SECTIONS="${1:-5}"

# Use awk to extract migration-related content
awk -v max="$MAX_SECTIONS" '
  BEGIN {
    count = 0
    in_section = 0
    section = ""
    section_header = ""
    in_code = 0
  }

  # Track code blocks
  /^```/ {
    in_code = !in_code
    if (in_section) {
      section = section "\n" $0
    }
    next
  }

  # Detect migration-related headers
  /[Bb]reaking [Cc]hange/ || /[Mm]igrat/ || /[Uu]pgrad/ || /[Vv]ersion [0-9]/ {
    if (!in_code && count < max) {
      # Start new section
      if (in_section && length(section) > 50) {
        count++
        print "### Migration " count ": " section_header
        print ""
        print section
        print ""
        print "---"
        print ""
      }
      section_header = $0
      gsub(/^#+[[:space:]]*/, "", section_header)
      section = ""
      in_section = 1
    }
    next
  }

  # Detect before/after patterns in code
  /\/\/ [Bb]efore/ || /\/\/ [Aa]fter/ || /# [Bb]efore/ || /# [Aa]fter/ {
    if (in_section) {
      section = section "\n" $0
    } else if (count < max) {
      # Start a before/after section
      section_header = "Code Migration"
      section = $0
      in_section = 1
    }
    next
  }

  # Accumulate section content
  in_section {
    section = section "\n" $0
  }

  # Section separator
  /^---+$/ && in_section {
    if (length(section) > 50 && count < max) {
      count++
      print "### Migration " count ": " section_header
      print ""
      print section
      print ""
    }
    section = ""
    section_header = ""
    in_section = 0
  }

  END {
    # Output last section
    if (in_section && length(section) > 50 && count < max) {
      count++
      print "### Migration " count ": " section_header
      print ""
      print section
      print ""
    }
  }
'
