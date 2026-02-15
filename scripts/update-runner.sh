#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
USER="$(id -un)"
SOCKET="/run/octo/runner-sockets/${USER}/octo-runner.sock"

# Stop running processes (use exact path match to avoid killing ourselves)
sudo killall -q octo-runner || true
sudo killall -q octo || true
sleep 1

# Install new binaries
sudo install -m 0755 "$ROOT_DIR/backend/target/release/octo" /usr/local/bin/octo
sudo install -m 0755 "$ROOT_DIR/backend/target/release/octo-runner" /usr/local/bin/octo-runner

# Restart
nohup /usr/local/bin/octo serve >/tmp/octo-serve.log 2>&1 &
nohup /usr/local/bin/octo-runner --socket "$SOCKET" >/tmp/octo-runner.log 2>&1 &

sleep 1
echo "octo and octo-runner updated and restarted"
pgrep -af "octo serve|octo-runner --socket"
