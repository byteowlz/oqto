#!/usr/bin/env bash
set -euo pipefail

# Guardrail (oqto-a9v9): st_dev is a mount-instance number on macOS. It changes
# whenever mount order changes, so a reboot silently broke every identity that
# persisted it (file grants, version 1 Session pins).
#
# Comparing two observations taken moments apart in one process is fine. Anything
# stored and compared later must use `oqto_personal::persistent_filesystem`
# (volume UUID on macOS) plus the inode.
#
# This lint fails when a file gains `.dev()` uses beyond the reviewed baseline, or
# a new file starts using it. Update the baseline only after confirming the new
# use is an in-process comparison, never a persisted identity.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
baseline="scripts/lint/mount-instance-baseline.txt"

current="$(rg -c '\.dev\(\)' backend/crates --glob '*.rs' --glob '!**/tests/**' --glob '!**/*tests.rs' --glob '!**/tests.rs' | sort)"
violations="$(join -a 1 -e 0 -o '0,1.2,2.2' -t: <(echo "$current") <(sort "$baseline") | awk -F: '$2 > $3 {print $1": "$2" uses, baseline "$3}')"

if [[ -n "$violations" ]]; then
  echo "[no-persisted-mount-instance] New st_dev (.dev()) uses; persisted identity must use persistent_filesystem:"
  echo "$violations"
  exit 1
fi
echo "[no-persisted-mount-instance] OK"
