#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source=${1:?qualified Pi source required}
daemon=${2:?explicit built daemon required}
bpf=${3:?explicit enforcing BPF required}
[[ $(git -C "$source" rev-parse HEAD) == cdaf6c227d4718a72b59ccd02152c9614df44cfc ]] || { echo 'Wrong Pi pin' >&2; exit 2; }
git -C "$source" diff --quiet HEAD --
cpu=$(python3 -c 'import os;print(min(os.sched_getaffinity(0)))')
export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR="$root/backend/target"
timeout --kill-after=5s 90s taskset -c "$cpu" nice -n 19 cargo build --offline --manifest-path "$root/backend/Cargo.toml" -p oq-permissions --example oqt_hands --example permission_carrier --example oqt_shim --example oqt_brain
cd "$root"
timeout --kill-after=5s 120s taskset -c "$cpu" nice -n 19 env OQT_SANDBOX_SHIM="$CARGO_TARGET_DIR/debug/examples/oqt_shim" OQTO_SANDBOX_SHIM_BIN="$CARGO_TARGET_DIR/debug/examples/oqt_shim" OQT_PI_SOURCE="$source" OQT_CARRIER="$CARGO_TARGET_DIR/debug/examples/permission_carrier" OQT_HANDS="$CARGO_TARGET_DIR/debug/examples/oqt_hands" OQT_BRAIN="$CARGO_TARGET_DIR/debug/examples/oqt_brain" OQT_ENV_DAEMON="$daemon" OQT_SECCOMP_BPF="$bpf" node --import "$source/packages/coding-agent/src/experimental/source-resolver.ts" --test --test-concurrency=1 tools/oqt/guard-input.test.mjs tools/oqt/guard-operator-client.test.mjs tools/oqt/guard-shell-safety.test.mjs tools/oqt/guard-safety.test.mjs tools/oqt/operator-receipts-composed.test.mjs tools/oqt/operator-receipts.test.mjs tools/oqt/hands.test.mjs tools/oqt/readonly-profile.test.mjs tools/oqt/credential-broker.test.mjs tools/oqt/owner-models.test.mjs tools/oqt/owner-model-runtime.test.mjs tools/oqt/confined-coding.test.mjs tools/oqt/gated-coding.test.mjs tools/oqt/hands-supervisor.test.mjs tools/oqt/coding-runtime.test.mjs tools/oqt/brain-confinement.test.mjs tools/oqt/owner-broker.test.mjs tools/oqt/resources.test.mjs
