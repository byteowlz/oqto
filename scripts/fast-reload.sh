#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Prompt for sudo once up-front
sudo -v

# Build on remote and fetch release binaries
cd "$ROOT_DIR/backend"
remote-build build --release -p oqto --bin oqto --bin oqto-runner
remote-build build --release -p oqto-files --bin oqto-files

# Install binaries system-wide
sudo install -m 0755 "$ROOT_DIR/backend/target/release/oqto" /usr/local/bin/oqto
sudo install -m 0755 "$ROOT_DIR/backend/target/release/oqto-runner" /usr/local/bin/oqto-runner
sudo install -m 0755 "$ROOT_DIR/backend/target/release/oqto-files" /usr/local/bin/oqto-files

# Restart oqto serve
pkill -f "oqto serve" || true
nohup /usr/local/bin/oqto serve >/tmp/oqto-serve.log 2>&1 &

# Restart runner
sudo pkill -f "/usr/local/bin/oqto-runner --socket /run/oqto/runner-sockets/$(id -un)/oqto-runner.sock" || true
mkdir -p "/run/oqto/runner-sockets/$(id -un)"
nohup /usr/local/bin/oqto-runner --socket "/run/oqto/runner-sockets/$(id -un)/oqto-runner.sock" >/tmp/oqto-runner.log 2>&1 &

echo "oqto binaries installed and services restarted"