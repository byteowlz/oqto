#!/usr/bin/env bash
# Deterministically stage, verify, and optionally promote an official Pi binary.
# No LLM and no package-manager resolution is involved.
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

for command in curl sha256sum tar timeout python3; do
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
  local binary="$1" label="$2" actual_version package_dir required_asset
  package_dir="$(cd "$(dirname "$binary")" && pwd -P)"
  for required_asset in theme/dark.json theme/light.json; do
    if [[ ! -r "$package_dir/$required_asset" ]]; then
      echo "pi-runtime: $label missing required asset: $required_asset" >&2
      return 1
    fi
  done

  actual_version="$($binary --version 2>/dev/null | head -1)"
  [[ "$actual_version" == "$VERSION" ]] || {
    echo "pi-runtime: $label version mismatch expected=$VERSION actual=${actual_version:-unavailable}" >&2
    return 1
  }

  # Keep stdin open until the correlated response arrives. A simple pipe can
  # deliver EOF during Pi startup and race the command in a clean CI HOME.
  python3 - "$binary" "${HOME:-}" <<'PY'
import json
import os
import pathlib
import selectors
import subprocess
import sys
import time

binary = sys.argv[1]
home = pathlib.Path(sys.argv[2]) if sys.argv[2] else None
# `-ne` (no extensions): verification is about the Pi *binary's* health
# (version + RPC model discovery), not the invoking user's extension state. A
# stale/conflicting user extension must not fail binary verification (it would
# abort startup with a tool conflict -> zero models). Provider checks below read
# auth.json/settings.json, which are independent of extensions.
proc = subprocess.Popen(
    [binary, "-ne", "--mode", "rpc", "--no-session"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
    text=True,
    bufsize=1,
    env=os.environ.copy(),
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
                and row.get("id") == "oqto-pi-runtime-smoke"
                and row.get("success") is True):
            candidate = (row.get("data") or {}).get("models")
            if isinstance(candidate, list):
                models = candidate
                break
finally:
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)

if not models:
    raise SystemExit("RPC model discovery returned no models")

providers = {str(model.get("provider", "")) for model in models}
if home:
    auth_path = home / ".pi" / "agent" / "auth.json"
    if auth_path.exists():
        try:
            auth = json.loads(auth_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            auth = {}
        if "openai-codex" in auth and "openai-codex" not in providers:
            raise SystemExit("configured openai-codex provider missing from RPC discovery")

    settings_path = home / ".pi" / "agent" / "settings.json"
    if settings_path.exists():
        try:
            settings = json.loads(settings_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            settings = {}
        packages = settings.get("packages") or []
        if any(str(package).split("@", 1)[0] == "npm:pi-claude-bridge" for package in packages):
            if "claude-bridge" not in providers:
                raise SystemExit("configured claude-bridge provider missing from RPC discovery")

print(f"models={len(models)} providers={len(providers)}")
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
