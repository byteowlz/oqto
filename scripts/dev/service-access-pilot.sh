#!/usr/bin/env bash
# Local trusted-channel CLI for the bounded pilot. Never accepts credentials as args.
set -euo pipefail

fail() {
  printf '{"schema":"oqto.service-access.v1","code":"cli_usage","message":"%s"}\n' "$1"
  exit 2
}
if [[ ${1:-} == --help ]]; then
  printf '%s\n' 'service-access-pilot.sh SOCKET inspect' \
    'service-access-pilot.sh SOCKET grant --apply < reviewed-grant.json' \
    'service-access-pilot.sh SOCKET revoke GRANT_ID --apply' \
    'Local admin Unix socket only. JSON stdout; nonzero exit on errors.' \
    'Grant credential is shown once: use umask 077 and a private output file.' \
    'No remote admin/JWT authority. No production mutation without parent review.'
  exit 0
fi
[[ $# -ge 2 ]] || fail 'Explicit Unix socket and operation required'
socket=$1
operation=$2
shift 2
[[ -S "$socket" ]] || fail 'Unix socket unavailable'
base='http://localhost/api/service-access/v1/grants'
request() {
  local output status
  if output=$(curl --silent --show-error --fail-with-body --max-time 10 \
    --unix-socket "$socket" "$@"); then
    printf '%s\n' "$output"
  else
    status=$?
    # HTTP/transport failures always have valid machine stdout. A timed-out
    # mutation may have committed: inspect by ID, never blindly retry issuance.
    printf '{"schema":"oqto.service-access.v1","code":"cli_request_failed"}\n'
    exit "$status"
  fi
}
case "$operation" in
  inspect)
    [[ $# -eq 0 ]] || fail 'Inspect accepts no arguments'
    request "$base"
    ;;
  grant)
    [[ $# -eq 1 && $1 == --apply ]] || fail 'Grant requires --apply and reviewed JSON on stdin'
    request -H 'Content-Type: application/json' --data-binary @- "$base"
    ;;
  revoke)
    [[ $# -eq 2 && $2 == --apply ]] || fail 'Revoke requires GRANT_ID --apply'
    [[ $1 =~ ^[A-Za-z0-9_-]{1,64}$ ]] || fail 'Invalid grant ID'
    request -X POST "$base/$1/revoke"
    ;;
  *) fail 'Unknown operation' ;;
esac
