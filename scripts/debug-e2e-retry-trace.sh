#!/usr/bin/env bash
set -euo pipefail

BASE_URL="http://localhost:3000"
USERNAME="wismut"
PASSWORD="dev"
WAIT_SHORT=6
WAIT_LONG=30
TRACE_DIR="/tmp/oqto-stream-traces"
MODELS="mock/mock/server-error,mock/mock/rate-limit"
OUT_DIR="/tmp/oqto-e2e-trace-$(date +%Y%m%dT%H%M%S)"

usage() {
  cat <<'EOF'
Usage:
  ./scripts/debug-e2e-retry-trace.sh [options]

Options:
  --base-url URL        Frontend URL (default: http://localhost:3000)
  --username USER       Login username (default: wismut)
  --password PASS       Login password (default: dev)
  --models CSV          Comma-separated models (default: mock/mock/server-error,mock/mock/rate-limit)
  --trace-dir DIR       Runner trace dir to harvest (default: /tmp/oqto-stream-traces)
  --out-dir DIR         Output directory (default: /tmp/oqto-e2e-trace-<timestamp>)
  --help                Show this help

Output artifacts:
  - screenshots per model + timepoint
  - frontend console dumps per model + timepoint
  - collected runner stream traces created during run
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-url) BASE_URL="$2"; shift 2 ;;
    --username) USERNAME="$2"; shift 2 ;;
    --password) PASSWORD="$2"; shift 2 ;;
    --models) MODELS="$2"; shift 2 ;;
    --trace-dir) TRACE_DIR="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
  esac
done

mkdir -p "$OUT_DIR"
start_ts="$(date +%s)"

ab() {
  DISPLAY=:0 agent-browser "$@"
}

snapshot_text() {
  ab snapshot -i
}

first_ref_matching() {
  local snapshot="$1"
  local needle="$2"
  python3 - "$needle" "$snapshot" <<'PY'
import re, sys
needle = sys.argv[1]
text = sys.argv[2].splitlines()
for line in text:
    if needle in line:
        m = re.search(r"ref=(e\d+)", line)
        if m:
            print(m.group(1))
            sys.exit(0)
sys.exit(1)
PY
}

first_ref_regex() {
  local snapshot="$1"
  local regex="$2"
  python3 - "$regex" "$snapshot" <<'PY'
import re, sys
pat = re.compile(sys.argv[1])
for line in sys.argv[2].splitlines():
    if pat.search(line):
        m = re.search(r"ref=(e\d+)", line)
        if m:
            print(m.group(1))
            sys.exit(0)
sys.exit(1)
PY
}

ab open "$BASE_URL/sessions"
ab wait 1500
snap="$(snapshot_text)"

if grep -q 'textbox "Username"' <<<"$snap"; then
  uref="$(first_ref_matching "$snap" 'textbox "Username"')"
  pref="$(first_ref_matching "$snap" 'textbox "Password"')"
  sref="$(first_ref_matching "$snap" 'button "Sign in"')"
  ab fill "@$uref" "$USERNAME"
  ab fill "@$pref" "$PASSWORD"
  ab click "@$sref"
  ab wait 2200
fi

ab eval "localStorage.setItem('debug:pi-v2','1')" >/dev/null

IFS=',' read -r -a model_arr <<<"$MODELS"

run_one_model() {
  local model="$1"
  local slug
  slug="$(sed 's#[^a-zA-Z0-9._-]#_#g' <<<"$model")"

  local prompt="trace-run ${model} $(date +%H:%M:%S)"

  local snap
  snap="$(snapshot_text)"
  local new_ref
  new_ref="$(first_ref_matching "$snap" 'button "New Session"')"
  ab click "@$new_ref"
  ab wait 1400

  snap="$(snapshot_text)"
  local model_box_ref
  model_box_ref="$(first_ref_regex "$snap" 'combobox .*Select model|combobox .*mock/mock|combobox .*/')"
  ab click "@$model_box_ref"
  ab wait 500

  snap="$(snapshot_text)"
  local search_ref
  search_ref="$(first_ref_matching "$snap" 'textbox "Search models..."')"
  ab fill "@$search_ref" "$model"
  ab wait 250

  snap="$(snapshot_text)"
  local option_ref
  option_ref="$(first_ref_matching "$snap" "option \"$model")"
  ab click "@$option_ref"
  ab wait 900

  snap="$(snapshot_text)"
  if ! grep -Fq "$model" <<<"$snap"; then
    echo "Failed to set model to $model" >&2
    return 1
  fi

  # Ensure the active agent process picks up the selected model before send.
  local restart_ref
  restart_ref="$(first_ref_matching "$snap" 'button "Restart agent"')"
  ab click "@$restart_ref"
  ab wait 3000

  snap="$(snapshot_text)"
  local input_ref
  input_ref="$(first_ref_matching "$snap" 'textbox "Type a message..."')"
  ab fill "@$input_ref" "$prompt"
  ab press Enter

  ab wait "$((WAIT_SHORT * 1000))"
  ab screenshot "$OUT_DIR/${slug}-t${WAIT_SHORT}s.png"
  ab console > "$OUT_DIR/${slug}-t${WAIT_SHORT}s.console.txt" || true
  ab snapshot -i > "$OUT_DIR/${slug}-t${WAIT_SHORT}s.snapshot.txt" || true
  ab eval "document.documentElement.outerHTML" > "$OUT_DIR/${slug}-t${WAIT_SHORT}s.dom.html" || true

  ab wait "$((WAIT_LONG * 1000))"
  ab screenshot "$OUT_DIR/${slug}-t$((WAIT_SHORT+WAIT_LONG))s.png"
  ab console > "$OUT_DIR/${slug}-t$((WAIT_SHORT+WAIT_LONG))s.console.txt" || true
  ab snapshot -i > "$OUT_DIR/${slug}-t$((WAIT_SHORT+WAIT_LONG))s.snapshot.txt" || true
  ab eval "document.documentElement.outerHTML" > "$OUT_DIR/${slug}-t$((WAIT_SHORT+WAIT_LONG))s.dom.html" || true

  ab reload
  ab wait 2500
  ab screenshot "$OUT_DIR/${slug}-reload.png"
  ab console > "$OUT_DIR/${slug}-reload.console.txt" || true
  ab snapshot -i > "$OUT_DIR/${slug}-reload.snapshot.txt" || true
  ab eval "document.documentElement.outerHTML" > "$OUT_DIR/${slug}-reload.dom.html" || true
}

for model in "${model_arr[@]}"; do
  run_one_model "$model"
done

# Collect runner stream traces produced during this run.
if [[ -d "$TRACE_DIR" ]]; then
  mkdir -p "$OUT_DIR/runner-traces"
  python3 - "$TRACE_DIR" "$start_ts" "$OUT_DIR/runner-traces" <<'PY'
import os, sys, shutil
trace_dir = sys.argv[1]
start_ts = int(sys.argv[2])
out = sys.argv[3]
for root, _, files in os.walk(trace_dir):
    for f in files:
        if not f.endswith('.jsonl'):
            continue
        p = os.path.join(root, f)
        try:
            mtime = int(os.path.getmtime(p))
        except OSError:
            continue
        if mtime >= start_ts:
            shutil.copy2(p, os.path.join(out, f))
PY
fi

echo "Trace artifacts written to: $OUT_DIR"
