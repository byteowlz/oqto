#!/usr/bin/env bash
set -euo pipefail

# Local file-mutation reliability checks over WS mux.
# Uses bun script scripts/e2e/reliability-files-mux.ts.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_SECRETS_FILE="${SCRIPT_DIR}/.secrets/octo-azure-reliability.local.secrets.toml"

SECRETS_FILE="$DEFAULT_SECRETS_FILE"
PROFILE="octo_azure"
LOOPS=5
WORKSPACE_PATH=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --secrets-file PATH   TOML secrets file (default: ${DEFAULT_SECRETS_FILE})
  --profile NAME        TOML table/profile name (default: octo_azure)
  --workspace-path PATH Override workspace path from secrets/auto-discovery
  --loops N             Mutation loops (default: 5)
  -h, --help            Show help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --secrets-file) SECRETS_FILE="$2"; shift 2 ;;
    --profile) PROFILE="$2"; shift 2 ;;
    --workspace-path) WORKSPACE_PATH="$2"; shift 2 ;;
    --loops) LOOPS="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

[[ -f "$SECRETS_FILE" ]] || { echo "Missing secrets file: $SECRETS_FILE" >&2; exit 1; }
command -v bun >/dev/null || { echo "bun not found" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq not found" >&2; exit 1; }

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
for key in ("base_url", "username", "password", "workspace_path"):
    val = cfg.get(key, "")
    if val is None:
        val = ""
    print(f"{key}={val}")
PY
)

BASE_URL=""
USERNAME=""
PASSWORD=""
CFG_WORKSPACE_PATH=""

for line in "${CFG[@]}"; do
  case "$line" in
    error=*) echo "Config error: ${line#error=}" >&2; exit 1 ;;
    base_url=*) BASE_URL="${line#base_url=}" ;;
    username=*) USERNAME="${line#username=}" ;;
    password=*) PASSWORD="${line#password=}" ;;
    workspace_path=*) CFG_WORKSPACE_PATH="${line#workspace_path=}" ;;
  esac
done

[[ -n "$BASE_URL" ]] || { echo "base_url missing" >&2; exit 1; }
[[ -n "$USERNAME" ]] || { echo "username missing" >&2; exit 1; }
[[ -n "$PASSWORD" ]] || { echo "password missing" >&2; exit 1; }
BASE_URL="${BASE_URL%/}"

if [[ -z "$WORKSPACE_PATH" ]]; then
  WORKSPACE_PATH="$CFG_WORKSPACE_PATH"
fi

login_payload=$(printf '{"username":"%s","password":"%s"}' "$USERNAME" "$PASSWORD")
login_resp=$(curl -sS -H 'Content-Type: application/json' -d "$login_payload" "${BASE_URL}/api/auth/login")
TOKEN=$(printf '%s' "$login_resp" | jq -r '.token // empty')
[[ -n "$TOKEN" ]] || { echo "Login failed: token missing" >&2; exit 1; }

if [[ -z "$WORKSPACE_PATH" ]]; then
  projects_resp=$(curl -sS -H "Authorization: Bearer ${TOKEN}" "${BASE_URL}/api/projects")
  WORKSPACE_PATH=$(printf '%s' "$projects_resp" | jq -r '.[0].path // empty')
fi

[[ -n "$WORKSPACE_PATH" ]] || { echo "workspace_path missing and auto-discovery failed" >&2; exit 1; }

echo "[files-mux] base_url=${BASE_URL} workspace_path=${WORKSPACE_PATH} loops=${LOOPS}"

bun run "${SCRIPT_DIR}/reliability-files-mux.ts" \
  --base-url "$BASE_URL" \
  --token "$TOKEN" \
  --workspace-path "$WORKSPACE_PATH" \
  --loops "$LOOPS"
