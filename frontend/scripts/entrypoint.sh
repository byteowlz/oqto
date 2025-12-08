#!/usr/bin/env bash
set -euo pipefail

OPENCODE_PORT=${OPENCODE_PORT:-4096}
FILE_SERVER_PORT=${FILE_SERVER_PORT:-9000}
TTYD_PORT=${TTYD_PORT:-9090}
OPENCODE_HOST=${OPENCODE_HOST:-0.0.0.0}
WORKSPACE_DIR=${WORKSPACE_DIR:-/workspace}

mkdir -p "${WORKSPACE_DIR}"
cd "${WORKSPACE_DIR}"

cleanup() {
  if [[ -n "${FILE_SERVER_PID:-}" ]] && kill -0 "${FILE_SERVER_PID}" 2>/dev/null; then
    kill "${FILE_SERVER_PID}" || true
  fi
  if [[ -n "${TTYD_PID:-}" ]] && kill -0 "${TTYD_PID}" 2>/dev/null; then
    kill "${TTYD_PID}" || true
  fi
}
trap cleanup EXIT

python3 -m http.server "${FILE_SERVER_PORT}" --bind 0.0.0.0 >/tmp/file-server.log 2>&1 &
FILE_SERVER_PID=$!

# Launch ttyd bound to the container shell
# --writable allows clipboard paste + writes, adjust as needed
TTYD_CMD=(ttyd --port "${TTYD_PORT}" --writable bash)
"${TTYD_CMD[@]}" >/tmp/ttyd.log 2>&1 &
TTYD_PID=$!

# Start opencode in the foreground
exec opencode serve -p "${OPENCODE_PORT}" --host "${OPENCODE_HOST}"
