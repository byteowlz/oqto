#!/usr/bin/env bash
set -euo pipefail

# Fix local single-user hstry/oqto stack after stale global hstry daemons.
# Usage:
#   bash scripts/fix-hstry-local.sh

UID_NOW="$(id -u)"
RUNTIME_SOCK="/run/user/${UID_NOW}/hstry.sock"
STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/hstry"
PORT_FILE="${STATE_DIR}/service.port"
PID_FILE="${STATE_DIR}/service.pid"

echo "[1/7] Stopping user services (oqto, oqto-runner, hstry)..."
systemctl --user stop oqto oqto-runner hstry || true

echo "[2/7] Killing stale global hstry daemons (requires sudo)..."
sudo pkill -f '/usr/local/bin/hstry service run' || true
sudo pkill -f 'hstry service run' || true

echo "[3/7] Cleaning stale runtime markers..."
rm -f "$PORT_FILE" "$PID_FILE" "$RUNTIME_SOCK"

echo "[4/7] Starting fresh user-owned hstry..."
systemctl --user start hstry

# Wait until systemd reports hstry active.
for i in {1..20}; do
  HSTRY_STATE="$(systemctl --user is-active hstry || true)"
  if [[ "$HSTRY_STATE" == "active" ]]; then
    break
  fi
  sleep 0.5
done

HSTRY_STATE="$(systemctl --user is-active hstry || true)"
echo "hstry systemd state: $HSTRY_STATE"
if [[ "$HSTRY_STATE" != "active" ]]; then
  echo "ERROR: hstry service did not become active"
  systemctl --user status hstry --no-pager || true
  exit 1
fi

# hstry may run with socket transport (no port file) or tcp transport (port file present).
if [[ -S "$RUNTIME_SOCK" ]]; then
  echo "hstry socket: $RUNTIME_SOCK"
fi
if [[ -f "$PORT_FILE" ]]; then
  echo "hstry port file: $(cat "$PORT_FILE")"
else
  echo "INFO: no hstry port file at $PORT_FILE (expected for socket transport)"
fi

echo "[5/7] Starting oqto-runner and oqto..."
systemctl --user start oqto-runner oqto

echo "[6/7] Verifying systemd services + backend health..."

# Wait for oqto-runner and oqto to leave "activating".
for i in {1..30}; do
  RUNNER_STATE="$(systemctl --user is-active oqto-runner || true)"
  OQTO_STATE="$(systemctl --user is-active oqto || true)"
  if [[ "$RUNNER_STATE" == "active" && "$OQTO_STATE" == "active" ]]; then
    break
  fi
  sleep 1
done

RUNNER_STATE="$(systemctl --user is-active oqto-runner || true)"
OQTO_STATE="$(systemctl --user is-active oqto || true)"
echo "oqto-runner state: $RUNNER_STATE"
echo "oqto state: $OQTO_STATE"

if [[ "$RUNNER_STATE" != "active" || "$OQTO_STATE" != "active" ]]; then
  echo "ERROR: oqto-runner/oqto failed to become active"
  systemctl --user status oqto-runner --no-pager || true
  systemctl --user status oqto --no-pager || true
  exit 1
fi

for i in {1..20}; do
  if curl -sf http://127.0.0.1:8080/api/health >/dev/null; then
    echo "oqto health OK"
    break
  fi
  sleep 1
done

if ! curl -sf http://127.0.0.1:8080/api/health >/dev/null; then
  echo "ERROR: oqto /api/health is not reachable"
  journalctl --user -u oqto -n 80 --no-pager || true
  exit 1
fi

echo "[7/7] Listing hstry service run processes..."
pgrep -af 'hstry service run' || true

echo "Done."
