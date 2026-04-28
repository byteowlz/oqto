#!/usr/bin/env bash
set -euo pipefail

# Guardrail: block reintroduction of hstry-as-authority read paths in runner/ws.
# Transitional fallback code is allowed only when explicitly marked with
# `legacy_fallback_ok` tag in comments.

violations="$(rg -n "source=hstry|get_session_messages_from_hstry\(|hstry client is not configured" backend/crates/oqto/src/runner backend/crates/oqto/src/api/ws_multiplexed -S | rg -v "legacy_fallback_ok" || true)"

if [[ -n "$violations" ]]; then
  echo "[no-legacy-history-authority] Found disallowed legacy authority patterns:"
  echo "$violations"
  exit 1
fi

echo "[no-legacy-history-authority] OK"
