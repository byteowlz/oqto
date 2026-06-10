#!/bin/bash
# Cross-platform Python detection
# Returns the correct Python command for the current system
# Handles Windows Microsoft Store alias stubs that don't actually work

detect_python() {
  # Try python3 first (Linux/macOS standard)
  # Must verify it actually works (Windows has fake python3 alias)
  if command -v python3 &> /dev/null; then
    if python3 --version &> /dev/null; then
      echo "python3"
      return 0
    fi
  fi

  # Try python (Windows standard, also works on some Linux)
  if command -v python &> /dev/null; then
    # Verify it actually works and is Python 3.x
    local version_output=$(python --version 2>&1)
    if [[ "$version_output" == Python\ 3* ]]; then
      echo "python"
      return 0
    fi
  fi

  # Try py launcher (Windows Python launcher)
  if command -v py &> /dev/null; then
    if py -3 --version &> /dev/null; then
      echo "py -3"
      return 0
    fi
  fi

  # Python not found
  return 1
}

# Check if Python is available and return appropriate message
check_python() {
  local python_cmd=$(detect_python)

  if [ -z "$python_cmd" ]; then
    echo "ERROR: Python 3 not found" >&2
    echo "" >&2
    echo "Please install Python 3:" >&2
    echo "  Windows: https://www.python.org/downloads/" >&2
    echo "  macOS:   brew install python3" >&2
    echo "  Linux:   sudo apt install python3" >&2
    return 1
  fi

  echo "$python_cmd"
  return 0
}
