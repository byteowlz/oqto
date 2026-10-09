#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="${ROOT_DIR}/.tmp"
STAMP_FILE="${STATE_DIR}/agent-check.last"
PROFILE="${1:-quick}"

mkdir -p "${STATE_DIR}"

current_sig="$(
  cd "${ROOT_DIR}" &&
    {
      printf 'profile=%s\n' "$PROFILE"
      git status --porcelain=v1 --untracked-files=normal |
        grep -Ev '^\?\? \.tmp/agent-check\.last$' || true
      git diff --binary
      git diff --cached --binary
      while IFS= read -r -d '' file; do
        [[ "$file" == .tmp/* ]] && continue
        printf '%s\0' "$file"
        if [[ -L "$file" ]]; then
          readlink -- "$file"
        elif [[ -f "$file" ]]; then
          sha256sum -- "$file"
        fi
      done < <(git ls-files --others --exclude-standard -z)
    } | sha256sum | awk '{print $1}'
)"
last_sig=""
if [[ -f "${STAMP_FILE}" ]]; then
  last_sig="$(cat "${STAMP_FILE}")"
fi

if [[ "${current_sig}" == "${last_sig}" ]]; then
  echo "[agent-check] no workspace changes since last successful run; skipping"
  exit 0
fi

echo "[agent-check] changes detected -> running ${PROFILE} gates"
cd "${ROOT_DIR}"

if [[ "${PROFILE}" == "full" ]]; then
  just lint
else
  ./scripts/lint/rust-ai-guardrails.sh
  (cd backend && cargo fmt --check)
  (cd frontend && bun run lint)
fi

echo "${current_sig}" > "${STAMP_FILE}"
echo "[agent-check] gates passed"
