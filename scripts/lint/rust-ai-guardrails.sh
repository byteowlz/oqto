#!/usr/bin/env bash
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

if ! command -v ast-grep >/dev/null 2>&1; then
  echo "error: ast-grep is required for AI guardrails (install with: cargo install ast-grep --locked)" >&2
  exit 2
fi

python3 -m unittest discover -s scripts/lint -p test_rust_test_context.py
python3 -m unittest discover -s scripts/lint -p test_rust_guardrail_paths.py

BASE_REF="${AIGENRIV_BASE_REF:-}"
if [[ -z "$BASE_REF" ]]; then
  if git rev-parse --verify origin/main >/dev/null 2>&1; then
    BASE_REF="$(git merge-base origin/main HEAD)"
  else
    BASE_REF="HEAD~1"
  fi
fi

paths_file="$(mktemp)"
trap 'rm -f "$paths_file"' EXIT
python3 scripts/lint/rust_guardrail_paths.py --base "$BASE_REF" > "$paths_file"
mapfile -d '' -t changed_rs < "$paths_file"

if [[ ${#changed_rs[@]} -eq 0 ]]; then
  echo "No changed Rust files to lint against AI guardrails."
  exit 0
fi

echo "Running ast-grep guardrails on ${#changed_rs[@]} changed Rust files (production scope)..."

report_output="$(./scripts/lint/rust-ai-guardrails-report.py --paths "${changed_rs[@]}" --exclude-cfg-test)"
echo "$report_output"

total="$(echo "$report_output" | awk -F'\t' '/^TOTAL\t/{print $2}')"
if [[ -z "$total" ]]; then
  echo "error: failed to parse TOTAL from guardrail report" >&2
  exit 2
fi

if [[ "$total" -gt 0 ]]; then
  echo "error: rust AI guardrail violations found in production scope: $total" >&2
  exit 1
fi

echo "Rust AI guardrails passed (production scope)."
