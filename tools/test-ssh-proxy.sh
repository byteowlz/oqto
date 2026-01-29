#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: tools/test-ssh-proxy.sh [--host <host>] [--no-host] [--octo-server <url>] [--profile <name>]

Runs octo-ssh-proxy and verifies that octo-sandbox can access SSH agent keys
via the proxy while ~/.ssh stays masked inside the sandbox.

Options:
  --host         Host to attempt an SSH connection (default: github.com).
  --no-host      Skip the SSH connection test.
  --octo-server  Octo server URL for approval prompts (default: http://localhost:8081).
  --profile      Sandbox profile name from config (default: development).
USAGE
}

host="github.com"
run_host_test=true
octo_server="http://localhost:8081"
profile="development"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      host="${2:-}"
      shift 2
      ;;
    --no-host)
      run_host_test=false
      shift 1
      ;;
    --octo-server)
      octo_server="${2:-}"
      shift 2
      ;;
    --profile)
      profile="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if ! command -v octo-ssh-proxy >/dev/null 2>&1; then
  echo "octo-ssh-proxy not found in PATH" >&2
  exit 1
fi

if ! command -v octo-sandbox >/dev/null 2>&1; then
  echo "octo-sandbox not found in PATH" >&2
  exit 1
fi

if [[ -z "${SSH_AUTH_SOCK:-}" ]]; then
  echo "SSH_AUTH_SOCK not set; starting ssh-agent."
  eval "$(ssh-agent -s)" >/dev/null
fi

if ! ssh-add -L >/dev/null 2>&1; then
  if [[ -f "$HOME/.ssh/id_ed25519" ]]; then
    ssh-add "$HOME/.ssh/id_ed25519" >/dev/null
  else
    echo "ssh-add -L failed and ~/.ssh/id_ed25519 not found." >&2
    echo "Start an ssh-agent and add a key first." >&2
    exit 1
  fi
fi

sock_root="$HOME/.config/octo"
mkdir -p "$sock_root"
sock_dir="$(mktemp -d "$sock_root/ssh-proxy-test.XXXXXX")"
proxy_sock="$sock_dir/octo-ssh.sock"

cleanup() {
  if [[ -n "${proxy_pid:-}" ]]; then
    kill "$proxy_pid" >/dev/null 2>&1 || true
    wait "$proxy_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$sock_dir"
}
trap cleanup EXIT

octo-ssh-proxy \
  --listen "$proxy_sock" \
  --upstream "$SSH_AUTH_SOCK" \
  --config "$HOME/.config/octo/sandbox.toml" \
  --octo-server "$octo_server" \
  --profile "$profile" \
  >/tmp/octo-ssh-proxy.log 2>&1 &
proxy_pid=$!

sleep 0.2

if ! kill -0 "$proxy_pid" >/dev/null 2>&1; then
  echo "octo-ssh-proxy failed to start. See /tmp/octo-ssh-proxy.log" >&2
  exit 1
fi

echo "Proxy running at $proxy_sock"

echo "Checking ~/.ssh inside sandbox (should be empty)"
octo-sandbox -- bash -lc 'ls -la ~/.ssh'

echo "Listing keys through proxy inside sandbox"
SSH_AUTH_SOCK="$proxy_sock" \
  octo-sandbox -- bash -lc 'ssh-add -L'

if [[ "$run_host_test" == "true" && -n "$host" ]]; then
  echo "Attempting SSH connection to $host (may prompt in Octo UI)"
  SSH_AUTH_SOCK="$proxy_sock" \
    octo-sandbox -- bash -lc "ssh -F /dev/null -o UserKnownHostsFile=/dev/null -o GlobalKnownHostsFile=/dev/null -o StrictHostKeyChecking=accept-new -T git@$host"
fi

echo "OK"
