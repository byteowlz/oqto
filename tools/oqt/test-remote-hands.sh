#!/usr/bin/env bash
# Remote hands (ADR-0086) through the real guard, oqt_link, pinned TLS,
# pairing, foreground oqto-hands-host and confined oqt_hands, on loopback.
# Usage: test-remote-hands.sh PI_SOURCE DAEMON BPF   (macOS: BPF is none)
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source=${1:?qualified Pi source required}
daemon=${2:?explicit built daemon required}
bpf=${3:?explicit enforcing BPF required (macOS: none)}
[[ $(git -C "$source" rev-parse HEAD) == cdaf6c227d4718a72b59ccd02152c9614df44cfc ]] || { echo 'Wrong Pi pin' >&2; exit 2; }
seccomp=()
if [[ $(uname -s) == Darwin ]]; then
 [[ "$bpf" == none ]] || { echo 'macOS has no seccomp: pass none' >&2; exit 2; }
else
 seccomp=(OQT_SECCOMP_BPF="$bpf")
fi
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-1} CARGO_TARGET_DIR="${OQT_TARGET_DIR:-$root/backend/target}"
timeout --kill-after=5s 600s cargo build --offline --manifest-path "$root/backend/Cargo.toml" -p oq-permissions --example oqt_hands --example permission_carrier --example oqt_shim --example oqt_brain
timeout --kill-after=5s 600s cargo build --offline --manifest-path "$root/backend/Cargo.toml" -p oqto-hands-link --bins
cd "$root"
bin=$CARGO_TARGET_DIR/debug
timeout --kill-after=5s 400s env "${seccomp[@]}" OQT_SANDBOX_SHIM="$bin/examples/oqt_shim" OQTO_SANDBOX_SHIM_BIN="$bin/examples/oqt_shim" OQT_PI_SOURCE="$source" OQT_CARRIER="$bin/examples/permission_carrier" OQT_HANDS="$bin/examples/oqt_hands" OQT_BRAIN="$bin/examples/oqt_brain" OQT_ENV_DAEMON="$daemon" OQT_LINK="$bin/oqt_link" OQTO_HANDS_HOST="$bin/oqto-hands-host" node --import "$source/packages/coding-agent/src/experimental/source-resolver.ts" --test --test-concurrency=1 tools/oqt/remote-hands.test.mjs tools/oqt/machine-tools.test.mjs
