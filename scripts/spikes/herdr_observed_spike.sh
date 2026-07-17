#!/usr/bin/env bash
set -euo pipefail

STATE_FILE="${OQTO_HERDR_SPIKE_STATE:-/tmp/oqto-herdr-observed-spike.json}"
DEFAULT_SESSION_ID="sess_spike_herdr_001"
DEFAULT_NAME="oqto-herdr-spike"
DEFAULT_CWD="/tmp"
DEFAULT_TASK="Herdr observed backend spike"

usage() {
  cat <<'USAGE'
Usage: herdr_observed_spike.sh <command> [args]

Commands:
  start [session-id] [cwd] [task]  Start a harmless shell harness through Herdr
  list                             Print Oqto-facing observed session read model
  read [lines]                     Read bounded terminal output by Oqto session id from state
  send <text>                      Send literal text plus newline to the observed session
  status                           Print Herdr status mapped into the read model
  attach                           Print attach descriptor/command (Herdr id stays internal)
  cleanup                          Close the Herdr pane for the spike session

This is intentionally a spike wrapper: Oqto session id is the only public key.
Herdr terminal/pane/workspace ids are stored as adapter-local bindings in STATE_FILE.
USAGE
}

require_state() {
  if [[ ! -s "$STATE_FILE" ]]; then
    echo "missing state: run '$0 start' first" >&2
    exit 1
  fi
}

herdr_target() {
  jq -r '.bindings.herdr.agent_label' "$STATE_FILE"
}

start() {
  local session_id="${1:-$DEFAULT_SESSION_ID}"
  local cwd="${2:-$DEFAULT_CWD}"
  local task="${3:-$DEFAULT_TASK}"
  local name="$DEFAULT_NAME"

  local result
  result=$(herdr agent start "$name" --cwd "$cwd" \
    --env AGENT_CTX_PLATFORM_NAME=oqto \
    --env "AGENT_CTX_PLATFORM_SESSION_ID=$session_id" \
    --env "AGENT_CTX_WORKSPACE_PATH=$cwd" \
    --env AGENT_CTX_HARNESS=shell \
    --env "AGENT_CTX_TASK=$task" \
    -- bash -lc 'echo READY; env | grep AGENT_CTX_ | sort; while IFS= read -r line; do echo GOT:$line; done')

  jq -n --argjson started "$result" --arg session_id "$session_id" --arg task "$task" --arg cwd "$cwd" --arg name "$name" '{
    oqto_session_id: $session_id,
    task: $task,
    repo: $cwd,
    harness: "shell",
    fidelity_tier: "observed",
    backend: "herdr",
    status: ($started.result.agent.agent_status // "unknown"),
    attach: { kind: "command", command: ("herdr agent attach " + $name) },
    bindings: {
      herdr: {
        internal: true,
        agent_label: $name,
        terminal_id: $started.result.agent.terminal_id,
        pane_id: $started.result.agent.pane_id,
        workspace_id: $started.result.agent.workspace_id,
        tab_id: $started.result.agent.tab_id
      }
    }
  }' > "$STATE_FILE"

  list
}

list() {
  require_state
  local target info status
  target=$(herdr_target)
  info=$(herdr agent get "$target")
  status=$(jq -r '.result.agent.agent_status // "unknown"' <<<"$info")
  jq --arg status "$status" '.status = $status | {oqto_session_id, task, repo, harness, fidelity_tier, backend, status, attach}' "$STATE_FILE"
}

read_output() {
  require_state
  local lines="${1:-40}"
  local session_id
  session_id=$(jq -r '.oqto_session_id' "$STATE_FILE")
  herdr agent read "$(herdr_target)" --source visible --lines "$lines" --format text \
    | jq --arg session_id "$session_id" '.result.read | {oqto_session_id: $session_id, format, source, truncated, text}'
}

send_text() {
  require_state
  if [[ $# -lt 1 ]]; then
    echo "send requires text" >&2
    exit 1
  fi
  herdr agent send "$(herdr_target)" "$*" >/dev/null
  herdr agent send "$(herdr_target)" $'\n' >/dev/null
  echo '{"sent":true}'
}

status() {
  require_state
  local target info
  target=$(herdr_target)
  info=$(herdr agent get "$target")
  jq --argjson info "$info" '.status = ($info.result.agent.agent_status // "unknown") | {oqto_session_id, status, backend, harness, fidelity_tier}' "$STATE_FILE"
}

attach() {
  require_state
  jq '{oqto_session_id, attach, note: "Herdr target is adapter-local; callers attach via Oqto session id and receive this descriptor."}' "$STATE_FILE"
}

cleanup() {
  require_state
  local pane_id
  pane_id=$(jq -r '.bindings.herdr.pane_id' "$STATE_FILE")
  herdr pane close "$pane_id" >/dev/null || true
  rm -f "$STATE_FILE"
  echo '{"cleaned":true}'
}

case "${1:-}" in
  start) shift; start "$@" ;;
  list) shift; list "$@" ;;
  read) shift; read_output "$@" ;;
  send) shift; send_text "$@" ;;
  status) shift; status "$@" ;;
  attach) shift; attach "$@" ;;
  cleanup) shift; cleanup "$@" ;;
  -h|--help|help|"") usage ;;
  *) usage >&2; exit 2 ;;
esac
