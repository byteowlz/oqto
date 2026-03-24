#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR/backend"

echo "[smoke] runner user-plane integration tests"
cargo test -p oqto integration_session_lifecycle_via_user_plane
cargo test -p oqto integration_file_and_memory_ops_via_user_plane
cargo test -p oqto integration_prompt_abort_retry_and_stream_subscription

echo "[smoke] completed"
