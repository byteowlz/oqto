#!/usr/bin/env bash
# Reuses a cargo-built oqto lib-test artifact. Private synthetic fixture only.
set -euo pipefail
umask 077
test_binary="${1:?Pass the existing oqto lib test executable}"
pi_binary="${2:-/tmp/oqto-pi-1-evidence/pi/pi}"
extensions="${3:-/home/wismut/byteowlz/pi-selection-development}"
hold="${OQTO_SELECTION_GATEWAY_HOLD_SECS:-0}"
[[ "$hold" =~ ^[0-9]{1,4}$ ]] && ((hold <= 7200)) || { printf '%s\n' 'Hold must be 0..7200 seconds' >&2; exit 2; }
root="$(mktemp -d /tmp/oqto-selection-auth-gateway.XXXXXX)"
home="$root/home"
mkdir -p "$home/.pi/agent" "$home/.config" "$home/.cache" "$home/.local/state" "$home/.local/share" "$home/runtime"
cleanup() {
 python3 - "$root" <<'PY'
import os,signal,sys
from pathlib import Path
root=Path(sys.argv[1]); pidfile=root/'child-pids'
if pidfile.exists():
 for text in pidfile.read_text().splitlines():
  pid=int(text)
  try:
   env=Path(f'/proc/{pid}/environ').read_bytes().split(b'\0')
   if f'HOME={root}/home'.encode() not in env: continue
   os.kill(pid, signal.SIGTERM)
  except (ProcessLookupError,FileNotFoundError): pass
PY
}
trap cleanup EXIT
printf '%s\n' "$root" > /tmp/oqto-selection-auth-gateway-last-root.txt
printf 'proof_root=%s\n' "$root"
git -C "$extensions" rev-parse HEAD > "$root/extension-head.txt"
test "$(git -C "$extensions" rev-parse HEAD)" = a9a87c3d2ba0d97573941b2834fcc8497d8d83b5
sha256sum "$pi_binary" > "$root/pi-binary.sha256"
# Disposable CA/server/client credentials; certificates never leave this fixture.
mkdir "$root/tls"
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$root/tls/ca.key" -out "$root/tls/ca.pem" -days 1 -subj '/CN=Selection fixture CA' > "$root/tls/generate.log" 2>&1
for role in server client; do
 openssl req -new -newkey rsa:2048 -nodes -keyout "$root/tls/$role.key" -out "$root/tls/$role.csr" -subj "/CN=fixture-$role" >> "$root/tls/generate.log" 2>&1
 if [ "$role" = server ]; then
  printf 'subjectAltName=DNS:fixture-server\nextendedKeyUsage=serverAuth\n' > "$root/tls/$role.ext"
 else
  printf 'extendedKeyUsage=clientAuth\n' > "$root/tls/$role.ext"
 fi
 openssl x509 -req -in "$root/tls/$role.csr" -CA "$root/tls/ca.pem" -CAkey "$root/tls/ca.key" -CAcreateserial -out "$root/tls/$role.pem" -days 1 -extfile "$root/tls/$role.ext" >> "$root/tls/generate.log" 2>&1
done
(
 cd "$root"
 env -i PATH=/usr/bin:/bin HOME="$home" \
 XDG_STATE_HOME="$home/.local/state" XDG_DATA_HOME="$home/.local/share" \
 XDG_CONFIG_HOME="$home/.config" XDG_CACHE_HOME="$home/.cache" XDG_RUNTIME_DIR="$home/runtime" \
 PI_CODING_AGENT_DIR="$home/.pi/agent" PI_OFFLINE=1 AGENT_BROWSER_SOCKET_DIR="$root/browser-sockets" \
 TMPDIR="$root" OQTO_SELECTION_PROOF_ROOT="$root" \
 OQTO_SELECTION_PI_BINARY="$pi_binary" OQTO_SELECTION_EXTENSION_ROOT="$extensions" \
 OQTO_SELECTION_GATEWAY_HOLD_SECS="$hold" \
 timeout --signal=TERM --kill-after=5 "$((hold+180))" "$test_binary" \
 --exact api::ws_multiplexed::auth_gateway_tests::authenticated_real_selection_gateway \
 --ignored --nocapture --test-threads=1
) 2>&1 | tee "$root/test.log"
