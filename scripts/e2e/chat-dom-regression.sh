#!/usr/bin/env bash
set -euo pipefail

# Deterministic chat regression test (DOM + API, with reload).
# Produces HTML/text dumps and fails on duplication/disappearance signals.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_URL="${BASE_URL:-http://localhost:3000}"
DISPLAY_VAR="${DISPLAY:-:0}"
TURNS=3
TIMEOUT_MS=60000
TOKEN="${TOKEN:-}"
USERNAME="${USERNAME:-}"
PASSWORD="${PASSWORD:-}"
USE_MOCK_DYNAMIC=false
ARTIFACT_DIR="${SCRIPT_DIR}/logs/chat-dom-regression-$(date -u +%Y%m%dT%H%M%SZ)"
SESSION_NAME="chat-dom-regression-$(date +%s)-$$"

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --base-url URL         Frontend URL (default: ${BASE_URL})
  --turns N              Number of prompts to send (default: ${TURNS})
  --timeout-ms N         Per-turn idle timeout (default: ${TIMEOUT_MS})
  --artifact-dir DIR     Output directory
  --token TOKEN          Auth token (preferred)
  --username USER        Login username (if token not provided)
  --password PASS        Login password (if token not provided)
  --mock-dynamic         Force model mock/dynamic and send ## triggers
  -h, --help             Show help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-url) BASE_URL="$2"; shift 2 ;;
    --turns) TURNS="$2"; shift 2 ;;
    --timeout-ms) TIMEOUT_MS="$2"; shift 2 ;;
    --artifact-dir) ARTIFACT_DIR="$2"; shift 2 ;;
    --token) TOKEN="$2"; shift 2 ;;
    --username) USERNAME="$2"; shift 2 ;;
    --password) PASSWORD="$2"; shift 2 ;;
    --mock-dynamic) USE_MOCK_DYNAMIC=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

command -v agent-browser >/dev/null || { echo "agent-browser not found" >&2; exit 1; }
mkdir -p "$ARTIFACT_DIR"
BASE_URL="${BASE_URL%/}"

ab() {
  DISPLAY="$DISPLAY_VAR" AGENT_BROWSER_SESSION="$SESSION_NAME" agent-browser "$@"
}

log() { printf '[chat-dom-regression] %s\n' "$*"; }

auth_login() {
  curl -sS -m 20 -X POST "${BASE_URL}/api/auth/login" \
    -H 'content-type: application/json' \
    -d "{\"username\":\"${USERNAME}\",\"password\":\"${PASSWORD}\"}" \
    | python3 -c 'import json,sys; print((json.load(sys.stdin) or {}).get("token", ""))'
}

if [[ -z "$TOKEN" ]]; then
  if [[ -n "$USERNAME" && -n "$PASSWORD" ]]; then
    TOKEN="$(auth_login)"
  fi
fi
[[ -n "$TOKEN" ]] || { echo "token missing: provide --token or --username/--password" >&2; exit 1; }

ab open "$BASE_URL/login" >/dev/null
ab eval "localStorage.setItem('oqto:authToken', '${TOKEN}'); localStorage.setItem('oqto:controlPlaneUrl', '${BASE_URL}'); 'ok'" >/dev/null
ab open "$BASE_URL/sessions" >/dev/null
ab wait 2500 >/dev/null
ab eval "window.__CHAT_DOM_E2E = { turns: ${TURNS}, timeoutMs: ${TIMEOUT_MS}, useMockDynamic: ${USE_MOCK_DYNAMIC} }; 'ok'" >/dev/null

JS_SEND="$(cat <<'JS'
(async () => {
  const cfg = window.__CHAT_DOM_E2E || {};
  const turns = Number.isFinite(cfg.turns) ? Number(cfg.turns) : 3;
  const timeoutMs = Number.isFinite(cfg.timeoutMs) ? Number(cfg.timeoutMs) : 60000;

  const manager = window.__octo_ws_manager__;
  if (!manager) throw new Error("ws-manager missing");

  const sid =
    [...manager.agentSessionHandlers.keys()].at(-1) ||
    [...manager.subscribedSessions.keys()].at(-1) ||
    [...manager.sessionReady.keys()].at(-1);
  if (!sid) throw new Error("No active session selected in UI");

  const input = document.querySelector('textarea[placeholder="Type a message..."]');
  if (!input) throw new Error("Chat input not found");

  const panelRoot = input.closest('.bg-card');
  if (!panelRoot) throw new Error("Chat panel root not found");

  const useMockDynamic = Boolean(cfg.useMockDynamic);
  const baseTs = Date.now();
  const prompts = useMockDynamic
    ? Array.from({ length: turns }, (_, i) => `##${String(i + 1).padStart(2, '0')}_${i === 0 ? 'intro' : i === 1 ? 'followup' : 'final'}`)
    : Array.from({ length: turns }, (_, i) => `E2E_DOM_${baseTs}_${i + 1}`);

  const waitForIdle = async () => {
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        unsub?.();
        reject(new Error("Timeout waiting for agent.idle"));
      }, timeoutMs);
      const unsub = manager.subscribeAgentSession(
        sid,
        (event) => {
          if (event?.event === 'agent.idle') {
            clearTimeout(timeout);
            unsub();
            resolve(null);
          }
        },
        undefined,
        { create: false },
      );
    });
  };

  for (const prompt of prompts) {
    manager.agentPrompt(sid, prompt);
    await waitForIdle();
  }

  await new Promise((r) => setTimeout(r, 500));
  return { sid, prompts };
})()
JS
)"

SEND_RESULT="$(ab eval "$JS_SEND")"
printf '%s\n' "$SEND_RESULT" > "$ARTIFACT_DIR/send-result.json"

# Force reload to catch disappearance-on-reload regressions.
ab eval "location.reload(); 'ok'" >/dev/null
ab wait 3000 >/dev/null

JS_DUMP="$(cat <<'JS'
(() => {
  const input = document.querySelector('textarea[placeholder="Type a message..."]');
  if (!input) throw new Error("Chat input not found after reload");
  const panelRoot = input.closest('.bg-card');
  if (!panelRoot) throw new Error("Chat panel root not found after reload");

  const panelText = (panelRoot.innerText || '').replace(/\s+/g, ' ').trim();
  const panelHtml = panelRoot.innerHTML || '';
  return { panelText, panelHtml };
})()
JS
)"

DUMP_RESULT="$(ab eval "$JS_DUMP")"
printf '%s\n' "$DUMP_RESULT" > "$ARTIFACT_DIR/dom-dump.json"

python3 - "$ARTIFACT_DIR/send-result.json" "$ARTIFACT_DIR/dom-dump.json" "$ARTIFACT_DIR" "$BASE_URL" "$TOKEN" <<'PY'
import json
import sys
import urllib.request
from pathlib import Path

send_path = Path(sys.argv[1])
dump_path = Path(sys.argv[2])
out_dir = Path(sys.argv[3])
base_url = sys.argv[4].rstrip("/")
token = sys.argv[5]

send = json.loads(send_path.read_text(encoding="utf-8"))
dump = json.loads(dump_path.read_text(encoding="utf-8"))

sid = send["sid"]
prompts = send["prompts"]
panel_text = dump.get("panelText", "")
panel_html = dump.get("panelHtml", "")

# DOM prompt occurrence counts (after reload)
prompt_dom_counts = {p: panel_text.count(p) for p in prompts}

# API prompt occurrence counts
req = urllib.request.Request(
    f"{base_url}/api/chat-history/{sid}/messages",
    headers={"Authorization": f"Bearer {token}"},
)
api_status = 0
api_messages = []
try:
    with urllib.request.urlopen(req, timeout=20) as resp:
        api_status = resp.status
        if resp.status == 200:
            api_messages = json.loads(resp.read().decode("utf-8"))
except Exception:
    api_status = 0

user_texts = []
for m in api_messages if isinstance(api_messages, list) else []:
    if m.get("role") != "user":
        continue
    for p in m.get("parts") or []:
        if p.get("type") == "text" and isinstance(p.get("text"), str):
            user_texts.append(p["text"])

prompt_api_counts = {p: sum(1 for t in user_texts if t == p) for p in prompts}

raw_json_markers = [
    '[{"type":"text"',
    '[{"text":',
    '{"type":"thinking"',
]
raw_json_hits = [m for m in raw_json_markers if m in panel_text]

failures = []
for p, c in prompt_dom_counts.items():
    if c != 1:
        failures.append(f"DOM prompt count for {p} is {c} (expected 1)")
for p, c in prompt_api_counts.items():
    if c != 1:
        failures.append(f"API prompt count for {p} is {c} (expected 1)")
if api_status != 200:
    failures.append(f"API status is {api_status} (expected 200)")
if raw_json_hits:
    failures.append("Raw JSON markers found in DOM text: " + ", ".join(raw_json_hits))

(out_dir / "panel.txt").write_text(panel_text, encoding="utf-8")
(out_dir / "panel.html").write_text(panel_html, encoding="utf-8")

summary = {
    "passed": not failures,
    "session_id": sid,
    "turns": len(prompts),
    "prompt_dom_counts": prompt_dom_counts,
    "prompt_api_counts": prompt_api_counts,
    "api_status": api_status,
    "api_message_count": len(api_messages) if isinstance(api_messages, list) else 0,
    "raw_json_hits": raw_json_hits,
    "failures": failures,
}
(out_dir / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
print(json.dumps(summary, indent=2))

if failures:
    raise SystemExit(1)
PY

log "PASS"
log "summary: $ARTIFACT_DIR/summary.json"
log "dom text: $ARTIFACT_DIR/panel.txt"
log "dom html: $ARTIFACT_DIR/panel.html"

ab close >/dev/null 2>&1 || true
