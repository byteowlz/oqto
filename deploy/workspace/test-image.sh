#!/usr/bin/env bash
set -euo pipefail

image="${1:-localhost/oqto-workspace:dev}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
manifest="$root/deploy/workspace/required-commands.txt"

commands=()
while IFS= read -r command; do
  [[ -z "$command" || "$command" == \#* ]] && continue
  commands+=("$command")
done <"$manifest"

podman run --rm --entrypoint sh "$image" -c '
  missing=0
  for command in "$@"; do
    if ! command -v "$command" >/dev/null 2>&1; then
      echo "missing: $command" >&2
      missing=1
    fi
  done
  test "$missing" -eq 0
  npm --version >/dev/null
  npx --version >/dev/null
  pi-bridge --help >/dev/null
  test -f /usr/local/bin/theme/dark.json
  test -f /usr/local/bin/theme/light.json
' sh "${commands[@]}"

# HOME is a volume in real placements; system-wide shell initialization must
# work without relying on image-layer user dotfiles.
podman run --rm --entrypoint bash "$image" --noprofile -ic '
  test "$STARSHIP_SHELL" = bash
  type z >/dev/null
  command -v starship >/dev/null
' >/dev/null 2>&1

# Prove the complete Pi runtime reaches RPC initialization. An empty model list
# is valid in this hermetic smoke (EAVS config is placement-provisioned); the
# response itself proves binary + adjacent runtime assets are coherent.
podman run --rm --entrypoint python3 "$image" -c '
import json, os, subprocess, tempfile, time

env = os.environ.copy()
env["HOME"] = tempfile.mkdtemp()
proc = subprocess.Popen(
    ["pi", "--mode", "rpc", "--no-session"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    text=True,
    env=env,
)
try:
    proc.stdin.write(json.dumps({"id": "workspace-image-smoke", "type": "get_available_models"}) + "\n")
    proc.stdin.flush()
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        line = proc.stdout.readline()
        if not line:
            if proc.poll() is not None:
                raise SystemExit(f"Pi exited before RPC response: {proc.returncode}")
            continue
        row = json.loads(line)
        if row.get("id") == "workspace-image-smoke":
            if row.get("success") is not True or not isinstance((row.get("data") or {}).get("models"), list):
                raise SystemExit(f"invalid Pi RPC response: {row}")
            break
    else:
        raise SystemExit("timed out waiting for Pi RPC response")
finally:
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)
'

echo "workspace image smoke: pass ($image)"
