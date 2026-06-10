#!/bin/bash
# Extract troubleshooting content (workarounds, debugging, errors, warnings)
# Targets: workaround, temporary, warning, error, fix, debug, issue patterns
# Usage: extract-troubleshooting.sh [max_sections] < input

set -euo pipefail

MAX_SECTIONS="${1:-5}"

# Use awk to extract troubleshooting-related content
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

  # Detect troubleshooting-related headers or content
  /[Ww]orkaround/ || /[Tt]emporary/ || /[Dd]ebug/ || /[Tt]roubleshoot/ || \
  /[Ff]ix[ing]?[[:space:]]/ || /[Ee]rror[[:space:]]/ || /[Ww]arning/ || \
  /[Ii]ssue/ || /[Pp]roblem/ || /[Ss]olution/ {
    if (!in_code) {
      if (in_section && length(section) > 50 && count < max) {
        # Output previous section
        count++
        print "### Troubleshooting " count
        if (section_header != "") {
          print "**" section_header "**"
          print ""
        }
        print section
        print ""
        print "---"
        print ""
      }
      # Start new section
      section_header = $0
      gsub(/^#+[[:space:]]*/, "", section_header)
      section = ""
      in_section = 1
    } else if (in_section) {
      section = section "\n" $0
    }
    next
  }

  # Accumulate section content when in a troubleshooting section
  in_section {
    section = section "\n" $0
  }

  # Section separator
  /^---+$/ {
    if (in_section && length(section) > 50 && count < max) {
      count++
      print "### Troubleshooting " count
      if (section_header != "") {
        print "**" section_header "**"
        print ""
      }
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
      print "### Troubleshooting " count
      if (section_header != "") {
        print "**" section_header "**"
        print ""
      }
      print section
      print ""
    }
  }
'
