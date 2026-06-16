#!/usr/bin/env bash
set -euo pipefail

TRACE_DIR="/tmp/oqto-stream-traces"
DISABLE=false
RESTART_OQTO=true

usage() {
  cat <<'EOF'
Usage:
  ./scripts/dev/restart-debug.sh [--trace-dir DIR] [--disable] [--runner-only]

Options:
  --trace-dir DIR   Trace output directory (default: /tmp/oqto-stream-traces)
  --disable         Disable runner stream tracing and restart runner
  --runner-only     Restart only oqto-runner (not oqto)
  --help            Show this help

Behavior:
  - Uses systemd user manager env:
      OQTO_TRACE_STREAMS=1
      OQTO_TRACE_DIR=<DIR>
  - Restarts oqto-runner so env takes effect.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --trace-dir) TRACE_DIR="$2"; shift 2 ;;
    --disable) DISABLE=true; shift ;;
    --runner-only) RESTART_OQTO=false; shift ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
  esac
done

if ! systemctl --user show-environment >/dev/null 2>&1; then
  echo "systemd --user is not available in this session; cannot configure debug env safely." >&2
  exit 1
fi

if [[ "$DISABLE" == "true" ]]; then
  systemctl --user unset-environment OQTO_TRACE_STREAMS OQTO_TRACE_DIR || true
  echo "Disabled runner stream tracing env"
else
  mkdir -p "$TRACE_DIR"
  systemctl --user set-environment OQTO_TRACE_STREAMS=1 OQTO_TRACE_DIR="$TRACE_DIR"
  echo "Enabled runner stream tracing: $TRACE_DIR"
fi

systemctl --user restart oqto-runner
if [[ "$RESTART_OQTO" == "true" ]]; then
  systemctl --user restart oqto || true
fi

echo "Done. Current env snippets:"
systemctl --user show-environment | rg '^OQTO_TRACE_' || true
