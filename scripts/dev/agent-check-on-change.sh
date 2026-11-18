#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="${ROOT_DIR}/.tmp"
STAMP_FILE="${STATE_DIR}/agent-check.last"
PROFILE="${1:-quick}"

# macOS ships shasum; Linux commonly ships sha256sum. Both produce SHA-256.
if command -v sha256sum >/dev/null 2>&1; then
  HASH_COMMAND=(sha256sum)
elif command -v shasum >/dev/null 2>&1; then
  HASH_COMMAND=(shasum -a 256)
else
  printf '%s\n' '[agent-check] SHA-256 tool unavailable (need sha256sum or shasum)' >&2
  exit 1
fi

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
          "${HASH_COMMAND[@]}" -- "$file"
        fi
      done < <(git ls-files --others --exclude-standard -z)
    } | "${HASH_COMMAND[@]}" | awk '{print $1}'
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
