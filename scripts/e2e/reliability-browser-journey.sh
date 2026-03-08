#!/usr/bin/env bash
set -euo pipefail

# Browser journey reliability checks (local-only)
# - Logs in via UI
# - Walks core app routes/tabs
# - Captures screenshots + console/errors per route
# - Optionally exercises media URL fetch + play/seek attempt

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_SECRETS_FILE="${SCRIPT_DIR}/.secrets/octo-azure-reliability.local.secrets.toml"
DEFAULT_PROFILE="octo_azure"
DISPLAY_VAR="${DISPLAY:-:0}"
SESSION_NAME="reliability-journey-$(date -u +%Y%m%dT%H%M%SZ)-$$"

SECRETS_FILE="$DEFAULT_SECRETS_FILE"
PROFILE="$DEFAULT_PROFILE"
SCREENSHOT_DIR=""
WAIT_MS=2500

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --secrets-file PATH     TOML secrets file (default: ${DEFAULT_SECRETS_FILE})
  --profile NAME          TOML table/profile name (default: ${DEFAULT_PROFILE})
  --screenshot-dir DIR    Output directory (default: scripts/e2e/logs/browser-journey-<ts>)
  --wait-ms N             Wait after each route open (default: 2500)
  -h, --help              Show help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --secrets-file) SECRETS_FILE="$2"; shift 2 ;;
    --profile) PROFILE="$2"; shift 2 ;;
    --screenshot-dir) SCREENSHOT_DIR="$2"; shift 2 ;;
    --wait-ms) WAIT_MS="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

[[ -f "$SECRETS_FILE" ]] || { echo "Missing secrets file: $SECRETS_FILE" >&2; exit 1; }
command -v agent-browser >/dev/null || { echo "agent-browser not found" >&2; exit 1; }

if [[ -z "$SCREENSHOT_DIR" ]]; then
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  SCREENSHOT_DIR="${SCRIPT_DIR}/logs/browser-journey-${ts}"
fi
mkdir -p "$SCREENSHOT_DIR"

# Parse TOML secrets
mapfile -t CFG < <(python3 - "$SECRETS_FILE" "$PROFILE" <<'PY'
import sys
from pathlib import Path
import json

try:
    import tomllib
except Exception as e:
    print(f"error=tomllib_unavailable:{e}")
    sys.exit(0)

path = Path(sys.argv[1])
profile = sys.argv[2]

try:
    data = tomllib.loads(path.read_text(encoding='utf-8'))
except Exception as e:
    print(f"error=toml_parse:{e}")
    sys.exit(0)

cfg = data.get(profile)
if not isinstance(cfg, dict):
    print("error=profile_missing")
    sys.exit(0)

for key in ("base_url", "username", "password", "workspace_path"):
    val = cfg.get(key, "")
    if val is None:
        val = ""
    print(f"{key}={val}")

media = cfg.get("media_paths", [])
if isinstance(media, list):
    for m in media:
        print(f"media_path={m}")
PY
)

BASE_URL=""
USERNAME=""
PASSWORD=""
WORKSPACE_PATH=""
MEDIA_PATHS=()

for line in "${CFG[@]}"; do
  case "$line" in
    error=*) echo "Config error: ${line#error=}" >&2; exit 1 ;;
    base_url=*) BASE_URL="${line#base_url=}" ;;
    username=*) USERNAME="${line#username=}" ;;
    password=*) PASSWORD="${line#password=}" ;;
    workspace_path=*) WORKSPACE_PATH="${line#workspace_path=}" ;;
    media_path=*) MEDIA_PATHS+=("${line#media_path=}") ;;
  esac
done

[[ -n "$BASE_URL" ]] || { echo "base_url missing" >&2; exit 1; }
[[ -n "$USERNAME" ]] || { echo "username missing" >&2; exit 1; }
[[ -n "$PASSWORD" ]] || { echo "password missing" >&2; exit 1; }

BASE_URL="${BASE_URL%/}"

ab() {
  DISPLAY="$DISPLAY_VAR" AGENT_BROWSER_SESSION="$SESSION_NAME" agent-browser "$@"
}

fail() {
  echo "[browser-journey] ERROR: $*" >&2
  exit 1
}

note() {
  echo "[browser-journey] $*"
}

capture_route_artifacts() {
  local name="$1"
  ab screenshot "${SCREENSHOT_DIR}/${name}.png" >/dev/null || true
  ab console > "${SCREENSHOT_DIR}/${name}.console.log" || true
  ab errors > "${SCREENSHOT_DIR}/${name}.errors.log" || true
}

route_check() {
  local route="$1"
  local name="$2"

  ab errors --clear >/dev/null || true
  ab console --clear >/dev/null || true

  ab open "${BASE_URL}${route}" >/dev/null
  ab wait "$WAIT_MS" >/dev/null

  local url
  url=$(ab get url | tr -d '"')
  if [[ "$url" == *"/login"* ]]; then
    capture_route_artifacts "$name"
    fail "Route ${route} redirected to login"
  fi

  capture_route_artifacts "$name"

  local info
  info=$(ab eval "(() => ({ ready: document.readyState, title: document.title, textLen: (document.body?.innerText || '').length }))()")
  printf '%s\n' "$info" > "${SCREENSHOT_DIR}/${name}.meta.json"

  note "route ${route} ok"
}

media_check() {
  [[ -n "$WORKSPACE_PATH" ]] || return 0
  [[ ${#MEDIA_PATHS[@]} -gt 0 ]] || return 0

  note "media checks started"
  local idx=0
  for rel in "${MEDIA_PATHS[@]}"; do
    idx=$((idx + 1))

    local media_url
    media_url=$(python3 - "$BASE_URL" "$WORKSPACE_PATH" "$rel" <<'PY'
import sys, urllib.parse
base = sys.argv[1].rstrip('/')
workspace = sys.argv[2]
rel = sys.argv[3]
q = urllib.parse.urlencode({"workspace_path": workspace, "path": rel})
print(f"{base}/api/workspace/files/file?{q}")
PY
)

    local js_url
    js_url=$(python3 - "$media_url" <<'PY'
import json,sys
print(json.dumps(sys.argv[1]))
PY
)

    local result
    result=$(ab eval "(async () => {
      const url = ${js_url};
      const out = { url, ok: false, status: 0, type: '', played: false, seeked: false, error: '' };
      try {
        const r = await fetch(url, { headers: { Range: 'bytes=0-2047' } });
        out.status = r.status;
        out.ok = r.status === 200 || r.status === 206;
        out.type = r.headers.get('content-type') || '';
        if (out.ok && (out.type.startsWith('audio/') || out.type.startsWith('video/'))) {
          const tag = out.type.startsWith('audio/') ? 'audio' : 'video';
          const m = document.createElement(tag);
          m.src = url;
          m.muted = true;
          m.preload = 'metadata';
          document.body.appendChild(m);
          await new Promise((resolve, reject) => {
            const t = setTimeout(resolve, 8000);
            m.onloadedmetadata = () => { clearTimeout(t); resolve(); };
            m.onerror = () => { clearTimeout(t); reject(new Error('media metadata load failed')); };
          });
          try {
            await m.play();
            out.played = true;
          } catch (_) {}
          try {
            const target = Number.isFinite(m.duration) && m.duration > 1 ? Math.min(1, m.duration - 0.1) : 0.5;
            m.currentTime = target;
            out.seeked = true;
          } catch (_) {}
          try { m.pause(); } catch (_) {}
        }
      } catch (e) {
        out.error = String(e && e.message ? e.message : e);
      }
      return out;
    })()")

    printf '%s\n' "$result" > "${SCREENSHOT_DIR}/media-${idx}.json"

    local ok status
    ok=$(python3 - "$result" <<'PY'
import json,sys
try:
  d=json.loads(sys.argv[1])
  print('1' if d.get('ok') else '0')
except Exception:
  print('0')
PY
)
    status=$(python3 - "$result" <<'PY'
import json,sys
try:
  d=json.loads(sys.argv[1]); print(d.get('status',0))
except Exception:
  print(0)
PY
)

    if [[ "$ok" != "1" ]]; then
      fail "media check failed for '${rel}' (status=${status})"
    fi

    note "media ${rel} ok (status=${status})"
  done
}

main() {
  note "artifacts: ${SCREENSHOT_DIR}"
  note "agent-browser session: ${SESSION_NAME}"

  # Login flow
  ab open "${BASE_URL}/login" >/dev/null
  ab wait "${WAIT_MS}" >/dev/null
  ab fill "input[placeholder='Enter your username']" "$USERNAME" >/dev/null
  ab fill "input[placeholder='Enter your password']" "$PASSWORD" >/dev/null
  ab click "button:has-text('Sign in')" >/dev/null
  ab wait "$WAIT_MS" >/dev/null

  local after_login_url
  after_login_url=$(ab get url | tr -d '"')
  if [[ "$after_login_url" == *"/login"* ]]; then
    capture_route_artifacts "login-failed"
    fail "Login failed; still on /login"
  fi
  capture_route_artifacts "after-login"
  note "login ok: ${after_login_url}"

  # Core app routes
  route_check "/dashboard" "route-dashboard"
  route_check "/sessions" "route-sessions"
  route_check "/agents" "route-agents"
  route_check "/settings" "route-settings"
  route_check "/sldr" "route-sldr"

  media_check

  ab close >/dev/null 2>&1 || true
  note "journey passed"
  note "screenshots/logs saved to ${SCREENSHOT_DIR}"
}

main "$@"
