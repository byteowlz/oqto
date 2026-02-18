#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
USER="$(id -un)"
SOCKET="/run/oqto/runner-sockets/${USER}/oqto-runner.sock"

# Stop running processes (use exact path match to avoid killing ourselves)
sudo killall -q oqto-runner || true
sudo killall -q oqto || true
sleep 1

# Install new binaries
sudo install -m 0755 "$ROOT_DIR/backend/target/release/oqto" /usr/local/bin/oqto
sudo install -m 0755 "$ROOT_DIR/backend/target/release/oqto-runner" /usr/local/bin/oqto-runner

# Restart
nohup /usr/local/bin/oqto serve >/tmp/oqto-serve.log 2>&1 &
nohup /usr/local/bin/oqto-runner --socket "$SOCKET" >/tmp/oqto-runner.log 2>&1 &

sleep 1
echo "oqto and oqto-runner updated and restarted"
pgrep -af "oqto serve|oqto-runner --socket"
