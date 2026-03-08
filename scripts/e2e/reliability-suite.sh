#!/usr/bin/env bash
set -euo pipefail

# Reliability suite for Oqto control-plane and core UX data paths.
#
# This script is intentionally API-first so it can run in CI/headless environments.
# It validates auth, shared workspace listing, chat history loading latency,
# and media/file URL reachability (preview path used by frontend tabs).
#
# Usage examples:
#   ./scripts/e2e/reliability-suite.sh \
#     --base-url https://oqto.engineeringautomation.eu \
#     --username admin --password 'secret' \
#     --shared-workspace-id sw_abc123 \
#     --workspace-path /home/oqto_shared_content-creation/oqto/OUTATIME \
#     --media-path gedankenexperiment_fliegen.txt
#
#   ./scripts/e2e/reliability-suite.sh --base-url http://127.0.0.1:8080 --username admin --password secret --duration-sec 900

BASE_URL=""
USERNAME=""
PASSWORD=""
SHARED_WORKSPACE_ID=""
WORKSPACE_PATH=""
DURATION_SEC=300
INTERVAL_SEC=5
CHAT_HISTORY_P95_BUDGET_MS=2000
SHARED_HISTORY_P95_BUDGET_MS=4000
QUIET=false

MEDIA_PATHS=()

log() {
  if [[ "$QUIET" == "false" ]]; then
    printf '[reliability] %s\n' "$*" >&2
  fi
}

die() {
  printf '[reliability] ERROR: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage: reliability-suite.sh [options]

Required:
  --base-url URL                Base URL, e.g. https://oqto.engineeringautomation.eu
  --username USER               Login username
  --password PASS               Login password

Optional:
  --shared-workspace-id ID      Shared workspace id for shared history checks
  --workspace-path PATH         Workspace path for file/media URL checks
  --media-path RELPATH          Relative media/file path in workspace (repeatable)
  --duration-sec N              Run duration in seconds (default: 300)
  --interval-sec N              Delay between loops (default: 5)
  --chat-p95-ms N               Personal chat history p95 budget (default: 2000)
  --shared-p95-ms N             Shared chat history p95 budget (default: 4000)
  --quiet                       Reduce logging
  --help                        Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-url) BASE_URL="$2"; shift 2 ;;
    --username) USERNAME="$2"; shift 2 ;;
    --password) PASSWORD="$2"; shift 2 ;;
    --shared-workspace-id) SHARED_WORKSPACE_ID="$2"; shift 2 ;;
    --workspace-path) WORKSPACE_PATH="$2"; shift 2 ;;
    --media-path) MEDIA_PATHS+=("$2"); shift 2 ;;
    --duration-sec) DURATION_SEC="$2"; shift 2 ;;
    --interval-sec) INTERVAL_SEC="$2"; shift 2 ;;
    --chat-p95-ms) CHAT_HISTORY_P95_BUDGET_MS="$2"; shift 2 ;;
    --shared-p95-ms) SHARED_HISTORY_P95_BUDGET_MS="$2"; shift 2 ;;
    --quiet) QUIET=true; shift ;;
    --help|-h) usage; exit 0 ;;
    *) die "Unknown arg: $1" ;;
  esac
done

[[ -n "$BASE_URL" ]] || die "--base-url is required"
[[ -n "$USERNAME" ]] || die "--username is required"
[[ -n "$PASSWORD" ]] || die "--password is required"

BASE_URL="${BASE_URL%/}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
COOKIE_JAR="$TMP_DIR/cookies.txt"
RESP_BODY="$TMP_DIR/resp.json"

chat_latencies=()
shared_latencies=()

percentile_95() {
  if [[ $# -eq 0 ]]; then
    echo 0
    return
  fi

  printf '%s\n' "$@" | sort -n > "$TMP_DIR/sorted.txt"
  local n
  n=$(wc -l < "$TMP_DIR/sorted.txt")
  local idx
  idx=$(( (95 * n + 99) / 100 ))
  if [[ $idx -lt 1 ]]; then idx=1; fi
  sed -n "${idx}p" "$TMP_DIR/sorted.txt"
}

json_get() {
  local expr="$1"
  python3 - "$expr" "$RESP_BODY" <<'PY'
import json, sys
expr = sys.argv[1]
path = sys.argv[2]
try:
    raw = open(path, 'r', encoding='utf-8').read().strip()
except Exception:
    print("")
    sys.exit(0)
if not raw:
    print("")
    sys.exit(0)
try:
    data = json.loads(raw)
except Exception:
    print("")
    sys.exit(0)
cur = data
for part in expr.split('.'):
    if not part:
        continue
    if isinstance(cur, dict):
        cur = cur.get(part)
    else:
        cur = None
        break
if cur is None:
    print("")
elif isinstance(cur, (dict, list)):
    print(json.dumps(cur))
else:
    print(cur)
PY
}

request_json() {
  local method="$1"
  local path="$2"
  local data="${3:-}"
  local auth_header="${4:-}"

  local url="${BASE_URL}${path}"
  local start_ms end_ms elapsed status
  start_ms=$(date +%s%3N)

  local curl_args=(
    -sS -m 20 -o "$RESP_BODY" -w '%{http_code}'
    -b "$COOKIE_JAR" -c "$COOKIE_JAR"
    -X "$method" "$url"
  )

  if [[ -n "$auth_header" ]]; then
    curl_args+=( -H "$auth_header" )
  fi

  if [[ -n "$data" ]]; then
    curl_args+=( -H 'Content-Type: application/json' -d "$data" )
  fi

  status=$(curl "${curl_args[@]}") || return 1

  end_ms=$(date +%s%3N)
  elapsed=$((end_ms - start_ms))
  printf '%s;%s\n' "$status" "$elapsed"
}

login() {
  local payload
  payload=$(printf '{"username":"%s","password":"%s"}' "$USERNAME" "$PASSWORD")
  local meta
  meta=$(request_json POST /api/auth/login "$payload") || die "Login request failed"
  local status elapsed
  status="${meta%%;*}"
  elapsed="${meta##*;}"
  [[ "$status" == "200" ]] || die "Login failed with HTTP $status"
  local token
  token=$(json_get "token")
  [[ -n "$token" ]] || die "Login response missing token"
  log "login ok (${elapsed}ms)"
  printf '%s' "$token"
}

check_health() {
  local code
  code=$(curl -sS -m 10 -o /dev/null -w '%{http_code}' "${BASE_URL}/api/health") || code="000"
  [[ "$code" == "200" ]] || die "Health check failed: HTTP $code"
}

check_me() {
  local auth="$1"
  local meta
  meta=$(request_json GET /api/me "" "$auth") || die "GET /api/me failed"
  local status
  status="${meta%%;*}"
  [[ "$status" == "200" ]] || die "GET /api/me HTTP $status"
}

check_chat_history() {
  local auth="$1"
  local meta status elapsed
  meta=$(request_json GET '/api/chat-history?limit=200' "" "$auth") || die "GET /api/chat-history failed"
  status="${meta%%;*}"
  elapsed="${meta##*;}"
  [[ "$status" == "200" ]] || die "GET /api/chat-history HTTP $status"
  chat_latencies+=("$elapsed")
}

check_shared_history() {
  local auth="$1"
  [[ -n "$SHARED_WORKSPACE_ID" ]] || return 0

  local meta status elapsed
  meta=$(request_json GET "/api/shared-workspaces/${SHARED_WORKSPACE_ID}/workdirs" "" "$auth") || die "GET shared workdirs failed"
  status="${meta%%;*}"
  [[ "$status" == "200" ]] || die "GET shared workdirs HTTP $status"

  meta=$(request_json GET "/api/chat-history?shared_workspace_id=${SHARED_WORKSPACE_ID}&limit=400" "" "$auth") || die "GET shared chat history failed"
  status="${meta%%;*}"
  elapsed="${meta##*;}"
  [[ "$status" == "200" ]] || die "GET shared chat history HTTP $status"
  shared_latencies+=("$elapsed")
}

check_media_urls() {
  local auth="$1"
  [[ -n "$WORKSPACE_PATH" ]] || return 0
  [[ ${#MEDIA_PATHS[@]} -gt 0 ]] || return 0

  local rel status
  for rel in "${MEDIA_PATHS[@]}"; do
    status=$(curl -sS -m 20 -o /dev/null -w '%{http_code}' \
      -H "$auth" \
      -H 'Range: bytes=0-1023' \
      --get \
      --data-urlencode "workspace_path=${WORKSPACE_PATH}" \
      --data-urlencode "path=${rel}" \
      "${BASE_URL}/api/workspace/files/file") || status="000"
    case "$status" in
      200|206)
        ;;
      *)
        die "Media/file URL check failed for '${rel}' with HTTP ${status}"
        ;;
    esac
  done
}

main() {
  log "starting reliability suite"
  log "base_url=${BASE_URL} duration_sec=${DURATION_SEC} interval_sec=${INTERVAL_SEC}"

  local token auth
  token=$(login)
  auth="Authorization: Bearer ${token}"

  local start now loops
  start=$(date +%s)
  loops=0

  while true; do
    now=$(date +%s)
    if (( now - start >= DURATION_SEC )); then
      break
    fi

    check_health
    check_me "$auth"
    check_chat_history "$auth"
    check_shared_history "$auth"
    check_media_urls "$auth"

    loops=$((loops + 1))
    if [[ "$QUIET" == "false" ]]; then
      log "loop=${loops} ok"
    fi

    sleep "$INTERVAL_SEC"
  done

  local p95_chat p95_shared
  p95_chat=$(percentile_95 "${chat_latencies[@]:-}")
  p95_shared=0
  if [[ ${#shared_latencies[@]} -gt 0 ]]; then
    p95_shared=$(percentile_95 "${shared_latencies[@]}")
  fi

  log "chat_history_p95_ms=${p95_chat} budget_ms=${CHAT_HISTORY_P95_BUDGET_MS}"
  if (( p95_chat > CHAT_HISTORY_P95_BUDGET_MS )); then
    die "chat history p95 ${p95_chat}ms exceeded budget ${CHAT_HISTORY_P95_BUDGET_MS}ms"
  fi

  if [[ ${#shared_latencies[@]} -gt 0 ]]; then
    log "shared_history_p95_ms=${p95_shared} budget_ms=${SHARED_HISTORY_P95_BUDGET_MS}"
    if (( p95_shared > SHARED_HISTORY_P95_BUDGET_MS )); then
      die "shared history p95 ${p95_shared}ms exceeded budget ${SHARED_HISTORY_P95_BUDGET_MS}ms"
    fi
  fi

  log "suite passed loops=${loops}"
}

main "$@"
