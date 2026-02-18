#!/usr/bin/env bash
# Diagnose message loss issues by comparing JSONL, hstry, and runner state

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SESSION_NAME="${1:-}"

if [[ -z "$SESSION_NAME" ]]; then
    echo "Usage: $0 <session-id-or-pattern>"
    echo "Example: $0 very-edge-lead"
    echo "         $0 98a530bd-5303-4db9-8fec-768f3ac2e39c"
    exit 1
fi

echo "=========================================="
echo "Message Loss Diagnostics for: $SESSION_NAME"
echo "=========================================="
echo ""

# 1. Find JSONL session file
echo "--- 1. JSONL Session File ---"
JSONL_FILE=$(find ~/.pi/agent/sessions -name "*.jsonl" -path "*$SESSION_NAME*" -printf '%T@ %p\n' 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2-)

if [[ -z "$JSONL_FILE" ]]; then
    # Try broader search
    JSONL_FILE=$(find ~/.pi/agent/sessions -name "*.jsonl" -mmin -60 -printf '%T@ %p\n' 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2-)
fi

if [[ -n "$JSONL_FILE" ]]; then
    echo "Found: $JSONL_FILE"
    echo "Size: $(stat -c%s "$JSONL_FILE" 2>/dev/null || stat -f%z "$JSONL_FILE" 2>/dev/null) bytes"
    echo "Modified: $(stat -c%y "$JSONL_FILE" 2>/dev/null || stat -f%Sm "$JSONL_FILE" 2>/dev/null)"
    echo ""
    echo "Last 10 messages in JSONL:"
    echo "(timestamp | role | preview)"
    tail -10 "$JSONL_FILE" | jq -r '[.timestamp, .message.role, (.message.content | if type == "string" then .[0:50] else (.[0].text // .[0].thinking // "[complex]")[0:50] end)] | @tsv' 2>/dev/null || tail -10 "$JSONL_FILE"
else
    echo "No JSONL file found for pattern: $SESSION_NAME"
fi

echo ""
echo "--- 2. hstry Database ---"
HSTRY_DB="${XDG_DATA_HOME:-$HOME/.local/share}/hstry/hstry.db"

if [[ -f "$HSTRY_DB" ]]; then
    echo "hstry DB: $HSTRY_DB"
    echo "Size: $(stat -c%s "$HSTRY_DB" 2>/dev/null || stat -f%z "$HSTRY_DB" 2>/dev/null) bytes"
    echo ""
    echo "Searching for session..."

    # Extract session UUID from JSONL filename if available
    if [[ -n "$JSONL_FILE" ]]; then
        SESSION_ID=$(basename "$JSONL_FILE" .jsonl | grep -oE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | tail -1)
        if [[ -n "$SESSION_ID" ]]; then
            echo "Extracted session ID: $SESSION_ID"
            echo ""
            echo "Messages in hstry (last 10):"
            sqlite3 "$HSTRY_DB" "
                SELECT m.idx, m.role, substr(m.parts, 1, 50) as preview
                FROM messages m
                JOIN conversations c ON m.conversation_id = c.id
                WHERE c.external_id = '$SESSION_ID'
                ORDER BY m.idx DESC
                LIMIT 10;
            " 2>/dev/null || echo "(sqlite3 query failed)"

            echo ""
            echo "Message count comparison:"
            JSONL_COUNT=$(grep -c '"type":"message"' "$JSONL_FILE" 2>/dev/null || echo "0")
            HSTRY_COUNT=$(sqlite3 "$HSTRY_DB" "
                SELECT COUNT(*)
                FROM messages m
                JOIN conversations c ON m.conversation_id = c.id
                WHERE c.external_id = '$SESSION_ID';
            " 2>/dev/null || echo "0")

            echo "JSONL messages: $JSONL_COUNT"
            echo "hstry messages: $HSTRY_COUNT"

            if [[ "$JSONL_COUNT" -ne "$HSTRY_COUNT" ]]; then
                echo ""
                echo "WARNING: Message count mismatch!"
                echo "Difference: $((JSONL_COUNT - HSTRY_COUNT))"
            fi
        fi
    fi
else
    echo "hstry DB not found at: $HSTRY_DB"
fi

echo ""
echo "--- 3. Active Runner Sessions ---"
# Try to query the runner
RUNNER_SOCKET="${XDG_RUNTIME_DIR:-/run/user/$(id - u)}/oqto/runner.sock"
if [[ -S "$RUNNER_SOCKET" ]]; then
    echo "Runner socket found: $RUNNER_SOCKET"
    # Could add oqtoctl command here if available
else
    echo "Runner socket not found at: $RUNNER_SOCKET"
fi

# Check tmux for running oqto-runner
echo ""
echo "Running oqto processes:"
pgrep -a oqto | head -10 || echo "(none found)"

echo ""
echo "--- 4. Frontend Connection ---"
echo "WebSocket connections (lsof -i :8080):"
lsof -i :8080 2>/dev/null | head -10 || echo "(no connections or lsof not available)"

echo ""
echo "--- 5. Log Analysis (last 5 minutes) ---"
LOG_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/oqto/logs"
if [[ -d "$LOG_DIR" ]]; then
    LATEST_LOG=$(find "$LOG_DIR" -name "*.log" -mmin -5 2>/dev/null | head -1)
    if [[ -n "$LATEST_LOG" ]]; then
        echo "Recent log: $LATEST_LOG"
        echo ""
        echo "Errors/Warnings in last 50 lines:"
        tail -50 "$LATEST_LOG" | grep -iE "(error|warn|fail|stall|timeout)" | tail -10 || echo "(no errors found)"
    else
        echo "No recent log files found"
    fi
else
    echo "Log directory not found"
fi

echo ""
echo "=========================================="
echo "Diagnostics complete"
echo "=========================================="
