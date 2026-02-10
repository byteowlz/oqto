#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Prompt for sudo once up-front
sudo -v

# Build on remote and fetch release binaries
cd "$ROOT_DIR/backend"
remote-build build --release -p octo --bin octo --bin octo-runner
remote-build build --release -p octo-files --bin octo-files

# Install binaries system-wide
sudo install -m 0755 "$ROOT_DIR/backend/target/release/octo" /usr/local/bin/octo
sudo install -m 0755 "$ROOT_DIR/backend/target/release/octo-runner" /usr/local/bin/octo-runner
sudo install -m 0755 "$ROOT_DIR/backend/target/release/octo-files" /usr/local/bin/octo-files

# Restart octo serve
pkill -f "octo serve" || true
nohup /usr/local/bin/octo serve >/tmp/octo-serve.log 2>&1 &

# Restart runner
sudo pkill -f "/usr/local/bin/octo-runner --socket /run/octo/runner-sockets/$(id -un)/octo-runner.sock" || true
mkdir -p "/run/octo/runner-sockets/$(id -un)"
nohup /usr/local/bin/octo-runner --socket "/run/octo/runner-sockets/$(id -un)/octo-runner.sock" >/tmp/octo-runner.log 2>&1 &

echo "octo binaries installed and services restarted"