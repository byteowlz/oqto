#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source=${1:?Usage: bash tools/adaptive-sandbox/run.sh /path/to/qualified/pi-source}
pin=cdaf6c227d4718a72b59ccd02152c9614df44cfc
source=$(cd "$source" && pwd)
[[ -d "$source/.git" || -f "$source/.git" ]] || { echo 'Expected qualified source checkout' >&2; exit 2; }
[[ $(git -C "$source" rev-parse HEAD) == "$pin" ]] || { echo 'Wrong upstream pin' >&2; exit 2; }
git -C "$source" diff --quiet HEAD -- || { echo 'Tracked upstream modifications refused' >&2; exit 2; }
[[ -f "$source/packages/durable/dist/index.js" ]] || { echo 'Build the qualified upstream source first' >&2; exit 2; }
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || { echo 'This confinement qualification lane requires Linux x86_64' >&2; exit 2; }
for command in bwrap cargo taskset nice timeout node python3; do command -v "$command" >/dev/null; done
cpu=$(python3 -c 'import os; print(min(os.sched_getaffinity(0)))')
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$root/backend/target"}
scratch=$(mktemp -d /tmp/oq-adaptive-gate.XXXXXX)
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/home" "$scratch/bpf"
# Every heavyweight phase has an explicit package/test filter and deadline.
bounded() { timeout --kill-after=5s 90s taskset -c "$cpu" nice -n 19 "$@"; }
cd "$root/backend"
bounded cargo fmt -p oq-permissions -- --check
bounded cargo build --offline -p oqto-sandbox --bin seccomp-policy-gen
bounded "$CARGO_TARGET_DIR/debug/seccomp-policy-gen" --policy crates/oqto/examples/seccomp/default.policy.toml --out-dir "$scratch/bpf" --arches x86_64
export OQ_ADAPTIVE_SECCOMP_BPF="$scratch/bpf/default-x86_64.bpf"
bounded cargo test --offline -p oq-permissions --lib --test lifecycle --test hands -- --test-threads=1
bounded cargo clippy --offline -p oq-permissions --all-targets -- -D warnings
bounded cargo build --offline -p oq-permissions --example permission_carrier
cd "$root"
bounded "$source/node_modules/.bin/biome" check tools/adaptive-sandbox/native.test.mjs
# Native qualification gets a clean profile/env, no credentials/live services.
bounded env -i PATH="$PATH" HOME="$scratch/home" LANG=C.UTF-8 \
    NODE_OPTIONS=--max-old-space-size=512 \
    OQ_ADAPTIVE_PI_SOURCE="$source" \
    OQ_ADAPTIVE_CARRIER="$CARGO_TARGET_DIR/debug/examples/permission_carrier" \
    OQ_ADAPTIVE_SECCOMP_BPF="$OQ_ADAPTIVE_SECCOMP_BPF" \
    node --conditions=import --test --test-concurrency=1 tools/adaptive-sandbox/native.test.mjs
# Child-process tests explicitly check teardown; catch any surviving carrier.
if pgrep -f "^$CARGO_TARGET_DIR/debug/examples/permission_carrier (tools|operator) " >/dev/null; then
    echo 'Unexpected surviving permission carrier; inspect before continuing' >&2
    exit 1
fi
