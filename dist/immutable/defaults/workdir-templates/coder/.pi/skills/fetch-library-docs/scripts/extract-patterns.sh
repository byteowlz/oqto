#!/bin/bash
# Extract best practices and design patterns content
# Targets: best practice, pattern, recommended, should, avoid, prefer patterns
# Usage: extract-patterns.sh [max_sections] < input

set -euo pipefail

MAX_SECTIONS="${1:-5}"

# Use awk to extract pattern/best practice content
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

  # Detect best practice patterns in headers or content
  /[Bb]est [Pp]ractice/ || /[Pp]attern/ || /[Rr]ecommend/ || \
  /[Ss]hould [Nn]ot/ || /[Aa]void/ || /[Pp]refer/ || \
  /[Ii]diomatic/ || /[Cc]onvention/ || /[Gg]uideline/ {
    if (!in_code) {
      if (in_section && length(section) > 50 && count < max) {
        # Output previous section
        count++
        print "### Pattern " count
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

  # Accumulate section content
  in_section {
    section = section "\n" $0
  }

  # Section separator
  /^---+$/ {
    if (in_section && length(section) > 50 && count < max) {
      count++
      print "### Pattern " count
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
      print "### Pattern " count
      if (section_header != "") {
        print "**" section_header "**"
        print ""
      }
      print section
      print ""
    }
  }
'
