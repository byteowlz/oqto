#!/usr/bin/env bash
set -euo pipefail

# Shared workspace lifecycle reliability checks (local-only)
# Creates, updates, validates, and deletes temporary shared workspaces.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_SECRETS_FILE="${SCRIPT_DIR}/.secrets/octo-azure-reliability.local.secrets.toml"

SECRETS_FILE="$DEFAULT_SECRETS_FILE"
PROFILE="octo_azure"
LOOPS=3
QUIET=false

log() {
  if [[ "$QUIET" == "false" ]]; then
    printf '[shared-reliability] %s\n' "$*"
  fi
}

die() {
  printf '[shared-reliability] ERROR: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --secrets-file PATH   TOML secrets file (default: ${DEFAULT_SECRETS_FILE})
  --profile NAME        TOML table/profile name (default: octo_azure)
  --loops N             Number of create/update/delete cycles (default: 3)
  --quiet               Reduce logging
  -h, --help            Show help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --secrets-file) SECRETS_FILE="$2"; shift 2 ;;
    --profile) PROFILE="$2"; shift 2 ;;
    --loops) LOOPS="$2"; shift 2 ;;
    --quiet) QUIET=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "Unknown arg: $1" ;;
  esac
done

[[ -f "$SECRETS_FILE" ]] || die "Missing secrets file: $SECRETS_FILE"

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
for key in ("base_url", "username", "password"):
    val = cfg.get(key, "")
    if val is None:
        val = ""
    print(f"{key}={val}")
PY
)

BASE_URL=""
USERNAME=""
PASSWORD=""

for line in "${CFG[@]}"; do
  case "$line" in
    error=*) die "Config error: ${line#error=}" ;;
    base_url=*) BASE_URL="${line#base_url=}" ;;
    username=*) USERNAME="${line#username=}" ;;
    password=*) PASSWORD="${line#password=}" ;;
  esac
done

[[ -n "$BASE_URL" ]] || die "base_url missing"
[[ -n "$USERNAME" ]] || die "username missing"
[[ -n "$PASSWORD" ]] || die "password missing"

BASE_URL="${BASE_URL%/}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
BODY="$TMP_DIR/body.json"

request() {
  local method="$1"
  local path="$2"
  local auth="$3"
  local data="${4:-}"

  local code
  if [[ -n "$data" ]]; then
    code=$(curl -sS -o "$BODY" -w '%{http_code}' \
      -H "$auth" -H 'Content-Type: application/json' \
      -X "$method" "${BASE_URL}${path}" -d "$data") || return 1
  else
    code=$(curl -sS -o "$BODY" -w '%{http_code}' \
      -H "$auth" \
      -X "$method" "${BASE_URL}${path}") || return 1
  fi
  printf '%s' "$code"
}

login_payload=$(printf '{"username":"%s","password":"%s"}' "$USERNAME" "$PASSWORD")
login_code=$(curl -sS -o "$BODY" -w '%{http_code}' -H 'Content-Type: application/json' -X POST "${BASE_URL}/api/auth/login" -d "$login_payload")
[[ "$login_code" == "200" ]] || die "Login failed HTTP ${login_code}"
TOKEN=$(jq -r '.token // empty' < "$BODY")
[[ -n "$TOKEN" ]] || die "Login token missing"
AUTH="Authorization: Bearer ${TOKEN}"

log "login ok"

for i in $(seq 1 "$LOOPS"); do
  suffix=$(printf '%04x' "$RANDOM")
  name="rel-sw-${suffix}-${i}"
  create_payload=$(printf '{"name":"%s","description":"Reliability lifecycle test","icon":"terminal","color":"#3ba77c"}' "$name")

  code=$(request POST /api/shared-workspaces "$AUTH" "$create_payload") || die "Create request failed"
  [[ "$code" == "200" || "$code" == "201" ]] || die "Create workspace failed HTTP ${code}"

  ws_id=$(jq -r '.id // empty' < "$BODY")
  [[ -n "$ws_id" ]] || die "Create workspace missing id"
  log "[$i/$LOOPS] created workspace id=${ws_id}"

  # Update workspace
  upd_name="${name}-upd"
  update_payload=$(printf '{"name":"%s","description":"Reliability lifecycle test updated"}' "$upd_name")
  code=$(request PATCH "/api/shared-workspaces/${ws_id}" "$AUTH" "$update_payload") || die "Update request failed"
  [[ "$code" == "200" ]] || die "Update workspace failed HTTP ${code}"
  got_name=$(jq -r '.name // empty' < "$BODY")
  [[ "$got_name" == "$upd_name" ]] || die "Updated name mismatch: got '${got_name}' expected '${upd_name}'"

  # Members/list checks
  code=$(request GET "/api/shared-workspaces/${ws_id}/members" "$AUTH") || die "List members failed"
  [[ "$code" == "200" ]] || die "List members HTTP ${code}"
  member_count=$(jq 'length' < "$BODY")
  [[ "$member_count" -ge 1 ]] || die "Expected at least 1 member"

  code=$(request GET "/api/shared-workspaces/${ws_id}/workdirs" "$AUTH") || die "List workdirs failed"
  [[ "$code" == "200" ]] || die "List workdirs HTTP ${code}"

  # Delete workspace and verify disappearance
  code=$(request DELETE "/api/shared-workspaces/${ws_id}" "$AUTH") || die "Delete request failed"
  [[ "$code" == "200" || "$code" == "204" ]] || die "Delete workspace HTTP ${code}"

  # eventual-consistency retry
  gone=false
  for _ in 1 2 3 4 5; do
    code=$(request GET "/api/shared-workspaces/${ws_id}" "$AUTH") || true
    if [[ "$code" == "404" ]]; then
      gone=true
      break
    fi
    sleep 1
  done
  [[ "$gone" == "true" ]] || die "Workspace ${ws_id} still retrievable after delete"

  log "[$i/$LOOPS] lifecycle passed"
done

log "all shared workspace lifecycle checks passed (loops=${LOOPS})"
