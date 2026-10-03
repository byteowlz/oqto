#!/usr/bin/env bash
# Main Pi process emulation, not physical-CPU or child-process CPU acceptance.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
[[ $# == 4 ]] || { echo 'Usage: test-node-pi-cpu.sh QEMU RUNTIME EXTENSION_REPO SHIPPED_EXTENSIONS' >&2; exit 2; }
emulator="$(realpath "$1")"
runtime="$(realpath "$2")"
repo="$(realpath "$3")"
extensions="$(realpath "$4")"
export DOCKER_HOST="${DOCKER_HOST:-unix:///var/run/docker.sock}"
docker run --rm --network none --user 65534:65534 --cap-drop ALL \
  --security-opt no-new-privileges --ulimit core=0 --read-only \
  --tmpfs /tmp:rw,exec,mode=1777 \
  -v "$emulator:/emulator:ro" -v "$runtime:/runtime:ro" \
  -v "$ROOT/scripts/dist:/proof:ro" -v "$repo:/extension-repo:ro" \
  -v "$extensions:/extensions:ro" \
  ubuntu@sha256:b1066385161d28ddf6bc7e7b28a9170eec11484c821d1a5150d176cbde41d7f7 \
  bash -lc '
    set -euo pipefail
    export HOME=/tmp/home
    mkdir "$HOME"
    printf "#!/bin/sh\nexec /emulator -cpu qemu64 /runtime/node/bin/node /runtime/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js \"\044@\"\n" > /tmp/pi-qemu
    chmod 755 /tmp/pi-qemu
    test "$(timeout -k 3 20 /runtime/pi --version)" = 1.0.0
    test "$(timeout -k 3 20 /tmp/pi-qemu --version)" = 1.0.0
    timeout -k 3 500 /runtime/node/bin/node /proof/test-shipped-extensions.mjs /tmp/pi-qemu /extensions
    timeout -k 3 100 /runtime/node/bin/node /extension-repo/scripts/test-selection-rpc.mjs /tmp/pi-qemu
    timeout -k 3 100 /runtime/node/bin/node /extension-repo/scripts/test-selection-tool.mjs /tmp/pi-qemu
  '
