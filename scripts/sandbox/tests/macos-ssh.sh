#!/usr/bin/env bash
# Disposable macOS key-grant proof. Never use the caller's keys or agent.
set -euo pipefail
if [[ "$(uname -s)" != Darwin ]]; then
  echo 'This proof requires macOS.' >&2
  exit 1
fi
root_dir="$(cd "$(dirname "$0")/../../.." && pwd)"
sandbox="${OQTO_SANDBOX_BIN:-$root_dir/backend/target/debug/oqto-sandbox}"
proxy="${OQTO_SSH_PROXY_BIN:-$root_dir/backend/target/debug/oqto-ssh-proxy}"
for executable in "$sandbox" "$proxy" /usr/bin/ssh-agent /usr/bin/ssh-add /usr/bin/ssh-keygen; do
  [[ -x "$executable" ]] || { echo "Missing executable: $executable" >&2; exit 1; }
done
fixture="$(mktemp -d /tmp/oq-ssh-XXXXXX)"
agent_pid=''
proxy_pid=''
cleanup() {
  for pid in "$proxy_pid" "$agent_pid"; do
    if [[ -n "$pid" ]]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
  done
  rm -rf "$fixture"
}
trap cleanup EXIT
mkdir -p "$fixture/home" "$fixture/keys" "$fixture/work"
export HOME="$fixture/home"
export SSH_AUTH_SOCK="$fixture/upstream.sock"
/usr/bin/ssh-agent -D -a "$SSH_AUTH_SOCK" > "$fixture/agent.log" 2>&1 &
agent_pid=$!
for _ in {1..100}; do [[ -S "$SSH_AUTH_SOCK" ]] && break; sleep 0.05; done
[[ -S "$SSH_AUTH_SOCK" ]]
for key in allowed denied; do
  /usr/bin/ssh-keygen -q -t ed25519 -N '' -C "oq-proof-$key" -f "$fixture/keys/$key"
  /usr/bin/ssh-add "$fixture/keys/$key" > /dev/null 2>&1
  cp "$fixture/keys/$key.pub" "$fixture/work/$key.pub"
done
"$proxy" --listen "$fixture/filtered.sock" --upstream "$SSH_AUTH_SOCK" \
  --allow-key "$fixture/keys/allowed.pub" --no-prompt > "$fixture/proxy.log" 2>&1 &
proxy_pid=$!
for _ in {1..100}; do [[ -S "$fixture/filtered.sock" ]] && break; sleep 0.05; done
[[ -S "$fixture/filtered.sock" ]]
# Explicit config in a disposable HOME, no system/user config lookup.
printf 'profile = "minimal"\nno_new_privs = false\nread_policy = "allowlist"\ndeny_read = ["%s", "%s"]\n' \
  "$fixture/keys" "$fixture/upstream.sock" > "$fixture/sandbox.toml"
run_sandbox() {
  "$sandbox" --config "$fixture/sandbox.toml" --workspace "$fixture/work" -- "$@"
}
export SSH_AUTH_SOCK="$fixture/filtered.sock"
identities="$(run_sandbox /usr/bin/ssh-add -L)"
[[ "$identities" == *oq-proof-allowed* && "$identities" != *oq-proof-denied* ]]
run_sandbox /usr/bin/ssh-add -T "$fixture/work/allowed.pub"
if run_sandbox /usr/bin/ssh-add -T "$fixture/work/denied.pub" > "$fixture/denied-sign.log" 2>&1; then
  echo 'FAIL: ungranted key could sign' >&2; exit 1
fi
if SSH_AUTH_SOCK="$fixture/upstream.sock" run_sandbox /usr/bin/ssh-add -L > "$fixture/upstream.log" 2>&1; then
  echo 'FAIL: direct upstream access' >&2; exit 1
fi
if run_sandbox /bin/cat "$fixture/keys/allowed" > "$fixture/key-read.log" 2>&1; then
  echo 'FAIL: private key readable' >&2; exit 1
fi
printf 'PASS: filtered identity listing, granted signing, ungranted signing denied, upstream socket denied, private key denied\n'
