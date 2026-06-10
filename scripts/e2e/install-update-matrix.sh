#!/usr/bin/env bash
set -euo pipefail

# Install/update validation matrix for oqto-vemr.8
#
# Default behavior is PLAN mode (prints commands). Use --execute to run.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
OQTO_SETUP_BIN="$ROOT_DIR/backend/target/release/oqto-setup"

EXECUTE="false"
PROFILE_SET="personal team"
TARGET="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
VERSION="matrix-$(date +%Y%m%d%H%M%S)"
ARTIFACT=""
CHECKSUM=""

usage() {
  cat <<EOF
Usage: $0 [options]

Options:
  --execute                 Execute commands (default: plan only)
  --profiles "..."          Profiles to validate (default: "personal team")
  --artifact FILE           Use prebuilt artifact instead of building via dist workflow
  --checksum FILE           Checksum file for --artifact
  --version LABEL           Artifact version label when building (default: matrix timestamp)
  -h, --help                Show help

Examples:
  $0
  $0 --execute --profiles "personal"
  $0 --execute --artifact dist/out/oqto-<v>-<target>.tar.gz --checksum dist/out/...sha256
EOF
}

run_cmd() {
  local cmd="$1"
  echo "[matrix] $cmd"
  if [[ "$EXECUTE" == "true" ]]; then
    bash -lc "$cmd"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --execute)
      EXECUTE="true"; shift ;;
    --profiles)
      PROFILE_SET="$2"; shift 2 ;;
    --artifact)
      ARTIFACT="$2"; shift 2 ;;
    --checksum)
      CHECKSUM="$2"; shift 2 ;;
    --version)
      VERSION="$2"; shift 2 ;;
    -h|--help)
      usage; exit 0 ;;
    *)
      echo "error: unknown arg: $1" >&2
      usage
      exit 1 ;;
  esac
done

if [[ -n "$ARTIFACT" && ! -f "$ARTIFACT" ]]; then
  echo "error: artifact not found: $ARTIFACT" >&2
  exit 1
fi
if [[ -n "$CHECKSUM" && ! -f "$CHECKSUM" ]]; then
  echo "error: checksum not found: $CHECKSUM" >&2
  exit 1
fi

if [[ "$EXECUTE" == "true" && ! -x "$OQTO_SETUP_BIN" ]]; then
  echo "error: oqto-setup binary not found at $OQTO_SETUP_BIN" >&2
  echo "run: just dist-stage-binaries --build" >&2
  exit 1
fi

if [[ -z "$ARTIFACT" ]]; then
  ARTIFACT="$ROOT_DIR/dist/out/oqto-${VERSION}-${TARGET}.tar.gz"
  CHECKSUM="${ARTIFACT}.sha256"
  run_cmd "just dist-sync"
  run_cmd "just dist-stage-binaries --build"
  run_cmd "just lint-dist-manifest-strict"
  run_cmd "just dist-package '$VERSION' '$TARGET'"
fi

if [[ "$EXECUTE" == "true" ]]; then
  if [[ ! -f "$ARTIFACT" ]]; then
    echo "error: artifact missing after build: $ARTIFACT" >&2
    exit 1
  fi
  if [[ ! -f "$CHECKSUM" ]]; then
    echo "error: checksum missing after build: $CHECKSUM" >&2
    exit 1
  fi
fi

for profile in $PROFILE_SET; do
  echo ""
  echo "=== MATRIX PROFILE: $profile ==="

  run_cmd "sudo '$OQTO_SETUP_BIN' install --artifact '$ARTIFACT' --checksum '$CHECKSUM'"

  # Authoritative gate: strict contract doctor.
  run_cmd "oqtoctl doctor --contract --profile '$profile' --strict"

  # Parity smoke check: setup wrapper should run doctor path too, but we only
  # retain summary lines to avoid duplicate full reports.
  if [[ "$profile" == "personal" ]]; then
    run_cmd "./setup.sh --personal --doctor | rg -n 'Contract finding summary|No inspected contract drift detected|setup contract drift detected'"
  else
    run_cmd "./setup.sh --team --doctor | rg -n 'Contract finding summary|No inspected contract drift detected|setup contract drift detected|Suggested remediation commands'"
  fi

done

echo ""
echo "Install/update matrix complete (mode: $([[ "$EXECUTE" == "true" ]] && echo execute || echo plan))."
