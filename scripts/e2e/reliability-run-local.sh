#!/usr/bin/env bash
set -euo pipefail

# Run reliability suite using local gitignored secrets.
#
# Default secret file:
#   scripts/e2e/.secrets/octo-azure-reliability.local.secrets.toml
#
# Expected TOML shape:
# [octo_azure]
# base_url = "https://..."
# username = "..."
# password = "..."
# shared_workspace_id = "sw_x"        # optional
# workspace_path = "/home/..."        # optional
# media_paths = ["a.mp3", "b.mp4"]   # optional

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_SECRETS_FILE="${SCRIPT_DIR}/.secrets/octo-azure-reliability.local.secrets.toml"
SUITE_SCRIPT="${SCRIPT_DIR}/reliability-suite.sh"

SECRETS_FILE="$DEFAULT_SECRETS_FILE"
PROFILE="octo_azure"
DURATION_SEC=""
INTERVAL_SEC=""
AUTO_DISCOVER=true
EXTRA_ARGS=()

usage() {
  cat <<EOF
Usage: $(basename "$0") [options] [-- extra reliability-suite args]

Options:
  --secrets-file PATH   TOML secrets file (default: ${DEFAULT_SECRETS_FILE})
  --profile NAME        TOML table/profile name (default: octo_azure)
  --duration-sec N      Override duration seconds
  --interval-sec N      Override interval seconds
  --no-auto-discover    Do not auto-discover shared workspace/workdir when missing
  -h, --help            Show help

Example:
  $0 --duration-sec 600 --interval-sec 5
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --secrets-file) SECRETS_FILE="$2"; shift 2 ;;
    --profile) PROFILE="$2"; shift 2 ;;
    --duration-sec) DURATION_SEC="$2"; shift 2 ;;
    --interval-sec) INTERVAL_SEC="$2"; shift 2 ;;
    --no-auto-discover) AUTO_DISCOVER=false; shift ;;
    --)
      shift
      EXTRA_ARGS+=("$@")
      break
      ;;
    -h|--help) usage; exit 0 ;;
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

[[ -f "$SECRETS_FILE" ]] || {
  echo "Missing secrets file: $SECRETS_FILE" >&2
  exit 1
}

[[ -x "$SUITE_SCRIPT" ]] || {
  echo "Missing or non-executable suite script: $SUITE_SCRIPT" >&2
  exit 1
}

# Parse TOML with Python 3.11+ tomllib and emit shell-safe lines.
mapfile -t CFG < <(python3 - "$SECRETS_FILE" "$PROFILE" <<'PY'
import sys
from pathlib import Path

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

for key in ("base_url", "username", "password", "shared_workspace_id", "workspace_path"):
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

if [[ ${#CFG[@]} -eq 0 ]]; then
  echo "Failed to parse secrets config" >&2
  exit 1
fi

BASE_URL=""
USERNAME=""
PASSWORD=""
SHARED_WORKSPACE_ID=""
WORKSPACE_PATH=""
MEDIA_ARGS=()

for line in "${CFG[@]}"; do
  case "$line" in
    error=*)
      echo "Config error: ${line#error=}" >&2
      exit 1
      ;;
    base_url=*) BASE_URL="${line#base_url=}" ;;
    username=*) USERNAME="${line#username=}" ;;
    password=*) PASSWORD="${line#password=}" ;;
    shared_workspace_id=*) SHARED_WORKSPACE_ID="${line#shared_workspace_id=}" ;;
    workspace_path=*) WORKSPACE_PATH="${line#workspace_path=}" ;;
    media_path=*) MEDIA_ARGS+=(--media-path "${line#media_path=}") ;;
  esac
done

[[ -n "$BASE_URL" ]] || { echo "base_url missing in profile '$PROFILE'" >&2; exit 1; }
[[ -n "$USERNAME" ]] || { echo "username missing in profile '$PROFILE'" >&2; exit 1; }
[[ -n "$PASSWORD" ]] || { echo "password missing in profile '$PROFILE'" >&2; exit 1; }

if [[ "$AUTO_DISCOVER" == "true" && ( -z "$SHARED_WORKSPACE_ID" || -z "$WORKSPACE_PATH" ) ]]; then
  login_json=$(curl -sS -H 'Content-Type: application/json' \
    -d "{\"username\":\"${USERNAME}\",\"password\":\"${PASSWORD}\"}" \
    "${BASE_URL}/api/auth/login" || true)
  token=$(printf '%s' "$login_json" | jq -r '.token // empty' 2>/dev/null || true)

  if [[ -n "$token" ]]; then
    auth_header="Authorization: Bearer ${token}"

    if [[ -z "$SHARED_WORKSPACE_ID" ]]; then
      sw_json=$(curl -sS -H "$auth_header" "${BASE_URL}/api/shared-workspaces" || true)
      SHARED_WORKSPACE_ID=$(printf '%s' "$sw_json" | jq -r '.[0].id // empty' 2>/dev/null || true)
    fi

    if [[ -n "$SHARED_WORKSPACE_ID" && -z "$WORKSPACE_PATH" ]]; then
      wd_json=$(curl -sS -H "$auth_header" "${BASE_URL}/api/shared-workspaces/${SHARED_WORKSPACE_ID}/workdirs" || true)
      WORKSPACE_PATH=$(printf '%s' "$wd_json" | jq -r '.[0].path // empty' 2>/dev/null || true)
    fi
  fi
fi

ARGS=(
  --base-url "$BASE_URL"
  --username "$USERNAME"
  --password "$PASSWORD"
)

if [[ -n "$SHARED_WORKSPACE_ID" ]]; then
  ARGS+=(--shared-workspace-id "$SHARED_WORKSPACE_ID")
fi
if [[ -n "$WORKSPACE_PATH" ]]; then
  ARGS+=(--workspace-path "$WORKSPACE_PATH")
fi
if [[ -n "$DURATION_SEC" ]]; then
  ARGS+=(--duration-sec "$DURATION_SEC")
fi
if [[ -n "$INTERVAL_SEC" ]]; then
  ARGS+=(--interval-sec "$INTERVAL_SEC")
fi

ARGS+=("${MEDIA_ARGS[@]}")
ARGS+=("${EXTRA_ARGS[@]}")

exec "$SUITE_SCRIPT" "${ARGS[@]}"
