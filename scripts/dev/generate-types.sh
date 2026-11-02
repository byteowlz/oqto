#!/usr/bin/env bash
# ts-rs export tests must actually exist; a zero-test cargo success is not proof.
set -euo pipefail
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-1}
listing=$(mktemp)
trap 'rm -f "$listing"' EXIT
cd "$repo/backend"
cargo test --locked -p oqto --lib export_bindings_ -- --list > "$listing"
if ! grep -Eq 'export_bindings_[^ ]*: test' "$listing"; then
    printf '%s\n' 'No ts-rs export tests found; refusing a zero-test type-generation success.' >&2
    exit 1
fi
cargo test --locked -p oqto --lib export_bindings_ -- --nocapture --test-threads=1
cd "$repo/frontend"
bun run format:generated-types
