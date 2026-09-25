#!/usr/bin/env bash
# Deterministically stage, verify, and optionally promote an official Pi binary.
# Provider login/model readiness is a later first-run gate, not a prerequisite
# for installing Pi on an unconfigured host. No LLM or package manager is used.
set -euo pipefail

VERSION=""
EXPECTED_SHA256=""
INSTALL_MODE=""
RUNTIME_ROOT="${PI_RUNTIME_ROOT:-}"
BASE_URL="${PI_RELEASE_BASE_URL:-https://github.com/earendil-works/pi/releases/download}"

usage() {
  cat <<'EOF'
Usage: pi-runtime.sh --version VERSION --sha256 SHA256 [--install|--install-user]
                     [--runtime-root PATH] [--base-url URL]

Without an install flag, verifies the candidate without changing the host.
--install promotes the root-owned system fallback under /var/lib/oqto.
--install-user promotes a rootless per-user runtime under XDG_DATA_HOME.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --sha256) EXPECTED_SHA256="$2"; shift 2 ;;
    --install) INSTALL_MODE="system"; shift ;;
    --install-user) INSTALL_MODE="user"; shift ;;
    --runtime-root) RUNTIME_ROOT="$2"; shift 2 ;;
    --base-url) BASE_URL="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || {
  echo "pi-runtime: invalid or missing version: ${VERSION:-<empty>}" >&2
  exit 2
}
[[ "$EXPECTED_SHA256" =~ ^[0-9a-f]{64}$ ]] || {
  echo "pi-runtime: invalid or missing SHA-256" >&2
  exit 2
}
if [[ -z "$RUNTIME_ROOT" ]]; then
  if [[ "$INSTALL_MODE" == "user" ]]; then
    RUNTIME_ROOT="${XDG_DATA_HOME:-${HOME:?HOME is required}/.local/share}/oqto/pi-runtimes"
  else
    RUNTIME_ROOT="/var/lib/oqto/pi-runtimes"
  fi
fi

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) ASSET="pi-linux-x64.tar.gz" ;;
  Linux-aarch64|Linux-arm64) ASSET="pi-linux-arm64.tar.gz" ;;
  *) echo "pi-runtime: unsupported deployment platform: $(uname -s)-$(uname -m)" >&2; exit 2 ;;
esac

for command in curl sha256sum tar timeout python3 env; do
  command -v "$command" >/dev/null || {
    echo "pi-runtime: required command not found: $command" >&2
    exit 1
  }
done

TMP="$(mktemp -d "${TMPDIR:-/tmp}/oqto-pi-runtime.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
ARCHIVE="$TMP/$ASSET"
RELEASE_URL="$BASE_URL/v$VERSION"

echo "pi-runtime: downloading $RELEASE_URL/$ASSET"
curl --fail --location --silent --show-error --retry 3 \
  "$RELEASE_URL/$ASSET" -o "$ARCHIVE"
ACTUAL_SHA256="$(sha256sum "$ARCHIVE" | awk '{print $1}')"
[[ "$ACTUAL_SHA256" == "$EXPECTED_SHA256" ]] || {
  echo "pi-runtime: checksum mismatch expected=$EXPECTED_SHA256 actual=$ACTUAL_SHA256" >&2
  exit 1
}

tar -xzf "$ARCHIVE" -C "$TMP"
CANDIDATE="$TMP/pi/pi"
[[ -x "$CANDIDATE" ]] || {
  echo "pi-runtime: archive does not contain executable pi/pi" >&2
  exit 1
}

verify_runtime() {
  local binary="$1" label="$2" actual_version
  mkdir -p "$TMP/probe-home/agent" "$TMP/probe-home/config" "$TMP/probe-home/data" "$TMP/probe-home/cache"
  # Even --version is executed without the invoking user's credentials or
  # profile: only the signed/verified candidate may inspect its scratch HOME.
  actual_version="$(env -i \
    HOME="$TMP/probe-home" \
    PI_CODING_AGENT_DIR="$TMP/probe-home/agent" \
    XDG_CONFIG_HOME="$TMP/probe-home/config" \
    XDG_DATA_HOME="$TMP/probe-home/data" \
    XDG_CACHE_HOME="$TMP/probe-home/cache" \
    PATH="${PATH:-/usr/bin:/bin}" \
    "$binary" --version 2>/dev/null | head -1)"
  [[ "$actual_version" == "$VERSION" ]] || {
    echo "pi-runtime: $label version mismatch expected=$VERSION actual=${actual_version:-unavailable}" >&2
    return 1
  }

  # Keep stdin open until the correlated response arrives. A simple pipe can
  # deliver EOF during Pi startup and race the command in a clean CI HOME.
  # Pi may create auth.json even for a model-list RPC: never point binary
  # verification at the operator's actual provider profile or environment.
  python3 - "$binary" "$TMP/probe-home" <<'PY'
import json
import os
import pathlib
import selectors
import subprocess
import sys
import time

binary = sys.argv[1]
probe_home = pathlib.Path(sys.argv[2])
probe_env = {
    "HOME": str(probe_home),
    "PI_CODING_AGENT_DIR": str(probe_home / "agent"),
    "XDG_CONFIG_HOME": str(probe_home / "config"),
    "XDG_DATA_HOME": str(probe_home / "data"),
    "XDG_CACHE_HOME": str(probe_home / "cache"),
    "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
}
# `-ne` (no extensions): verify Pi's binary version and RPC transport without
# importing arbitrary extensions from the invoking user's profile. A fresh
# user legitimately has no models until provider OAuth/API-key setup. Model
# availability and real chat belong to a separate post-login readiness gate.
proc = subprocess.Popen(
    [binary, "-ne", "--mode", "rpc", "--no-session"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
    text=True,
    bufsize=1,
    env=probe_env,
)
models = None
try:
    assert proc.stdin is not None and proc.stdout is not None
    proc.stdin.write(json.dumps({"id": "oqto-pi-runtime-smoke", "type": "get_available_models"}) + "\n")
    proc.stdin.flush()
    selector = selectors.DefaultSelector()
    selector.register(proc.stdout, selectors.EVENT_READ)
    deadline = time.monotonic() + 40
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            break
        ready = selector.select(timeout=min(0.5, deadline - time.monotonic()))
        if not ready:
            continue
        line = proc.stdout.readline()
        if not line:
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (row.get("type") == "response"
                and row.get("id") == "oqto-pi-runtime-smoke"):
            if row.get("success") is not True:
                raise SystemExit("RPC model discovery response failed")
            candidate = (row.get("data") or {}).get("models")
            if not isinstance(candidate, list):
                raise SystemExit("RPC model discovery response has invalid models")
            models = candidate
            break
finally:
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)

if models is None:
    raise SystemExit("RPC model discovery response was unavailable")

print(f"RPC verified; models reported={len(models)} (provider readiness not checked)")
PY
  echo "pi-runtime: $label verified (version=$actual_version)"
}

verify_runtime "$CANDIDATE" "candidate"

if [[ -z "$INSTALL_MODE" ]]; then
  echo "pi-runtime: candidate passed; no host changes requested"
  exit 0
fi

RELEASE_DIR="$RUNTIME_ROOT/$VERSION"
STAGE_DIR="$RUNTIME_ROOT/.staging-$VERSION-$$"
CURRENT_LINK="$RUNTIME_ROOT/current"
PREVIOUS_TARGET="$(readlink "$CURRENT_LINK" 2>/dev/null || true)"

if [[ "$INSTALL_MODE" == "system" ]]; then
  PRIVILEGED=(sudo)
else
  PRIVILEGED=()
fi

"${PRIVILEGED[@]}" install -d -m 0755 "$RUNTIME_ROOT"
"${PRIVILEGED[@]}" rm -rf "$STAGE_DIR"
"${PRIVILEGED[@]}" cp -a "$TMP/pi" "$STAGE_DIR"
"${PRIVILEGED[@]}" chmod -R a+rX "$STAGE_DIR"
printf '{"version":"%s","asset":"%s","archive_sha256":"%s"}\n' \
  "$VERSION" "$ASSET" "$EXPECTED_SHA256" \
  | "${PRIVILEGED[@]}" tee "$STAGE_DIR/oqto-runtime.json" >/dev/null

# Preserve a verified immutable version directory. If an earlier interrupted
# install left the same version corrupt, quarantine it rather than deleting
# evidence, then promote the freshly verified staging directory.
if [[ -d "$RELEASE_DIR" ]] && verify_runtime "$RELEASE_DIR/pi" "existing runtime"; then
  "${PRIVILEGED[@]}" rm -rf "$STAGE_DIR"
else
  if [[ -e "$RELEASE_DIR" ]]; then
    "${PRIVILEGED[@]}" mv "$RELEASE_DIR" "$RUNTIME_ROOT/.invalid-$VERSION-$(date +%s)"
  fi
  "${PRIVILEGED[@]}" mv "$STAGE_DIR" "$RELEASE_DIR"
fi

NEW_LINK="$RUNTIME_ROOT/.current-$$"
"${PRIVILEGED[@]}" ln -s "$VERSION" "$NEW_LINK"
"${PRIVILEGED[@]}" mv -Tf "$NEW_LINK" "$CURRENT_LINK"
if [[ "$INSTALL_MODE" == "system" ]]; then
  sudo ln -sfn "$CURRENT_LINK/pi" /usr/local/bin/pi
fi

if ! verify_runtime "$CURRENT_LINK/pi" "promoted runtime"; then
  echo "pi-runtime: promoted runtime verification failed; rolling back" >&2
  if [[ -n "$PREVIOUS_TARGET" ]]; then
    ROLLBACK_LINK="$RUNTIME_ROOT/.rollback-$$"
    "${PRIVILEGED[@]}" ln -s "$PREVIOUS_TARGET" "$ROLLBACK_LINK"
    "${PRIVILEGED[@]}" mv -Tf "$ROLLBACK_LINK" "$CURRENT_LINK"
  else
    "${PRIVILEGED[@]}" rm -f "$CURRENT_LINK"
  fi
  exit 1
fi

echo "pi-runtime: promoted $VERSION (previous=${PREVIOUS_TARGET:-none})"
