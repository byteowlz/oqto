#!/usr/bin/env bash
# Uses an already-built oqto-runner lib test binary; never installs or touches real HOME.
set -euo pipefail
umask 077
test_binary="${1:?Pass the cargo-built oqto_runner lib test executable}"
pi_binary="${2:-/tmp/oqto-pi-1-evidence/pi/pi}"
extensions="${3:-/home/wismut/byteowlz/pi-selection-development}"
root="$(mktemp -d /tmp/oqto-selection-real-runner.XXXXXX)"
home="$root/home"
mkdir -p "$home/.pi/agent" "$home/.config" "$home/.cache" "$home/.local/state" "$home/.local/share"
cleanup() {
    # Kill only this fixture's surviving Pi children, checking their private HOME.
    python3 - "$root" <<'PY'
import os,signal,sys,time
from pathlib import Path
root=Path(sys.argv[1]); pidfile=root/'child-pids'
if pidfile.exists():
 for text in pidfile.read_text().splitlines():
  pid=int(text)
  try:
   env=Path(f'/proc/{pid}/environ').read_bytes().split(b'\0')
   if f'HOME={root}/home'.encode() not in env: continue
   os.kill(pid, signal.SIGTERM)
  except ProcessLookupError: pass
  except FileNotFoundError: pass
PY
}
trap cleanup EXIT
printf '%s\n' "$root" > /tmp/oqto-selection-real-last-root.txt
printf 'proof_root=%s\n' "$root"
git -C "$extensions" rev-parse HEAD > "$root/extension-head.txt"
env -i PATH=/usr/bin:/bin HOME="$home" PI_CODING_AGENT_DIR="$home/.pi/agent" \
 XDG_STATE_HOME="$home/.local/state" XDG_DATA_HOME="$home/.local/share" \
 XDG_CONFIG_HOME="$home/.config" XDG_CACHE_HOME="$home/.cache" PI_OFFLINE=1 \
 "$pi_binary" --version > "$root/pi-version.txt"
sha256sum "$pi_binary" > "$root/pi-binary.sha256"
(
 cd "$root"
 env -i PATH=/usr/bin:/bin HOME="$home" \
  XDG_STATE_HOME="$home/.local/state" XDG_DATA_HOME="$home/.local/share" \
  XDG_CONFIG_HOME="$home/.config" XDG_CACHE_HOME="$home/.cache" \
  PI_CODING_AGENT_DIR="$home/.pi/agent" PI_OFFLINE=1 \
  TMPDIR="$root" OQTO_SELECTION_PROOF_ROOT="$root" \
  OQTO_SELECTION_PI_BINARY="$pi_binary" OQTO_SELECTION_EXTENSION_ROOT="$extensions" \
  timeout --signal=TERM --kill-after=5 120 "$test_binary" \
  --exact pi_manager::control::real_tests::real_pi_one_serialized_runner_selection_acceptance \
  --ignored --nocapture --test-threads=1
) 2>&1 | tee "$root/test.log"
