#!/usr/bin/env bash
# CPU emulation startup probe, not physical-CPU/model readiness acceptance.
set -euo pipefail
emulator="$(realpath "${1:?path to qemu-x86_64-static}")"
runtime="$(realpath "${2:?extracted official runtime directory containing pi executable}")"
cpu="${3:?explicit QEMU CPU model (e.g. Westmere)}"
[[ -x "$emulator" && -x "$runtime/pi" ]]
export DOCKER_HOST="${DOCKER_HOST:-unix:///var/run/docker.sock}"
echo "CPU startup probe: $cpu"
"$emulator" --version | head -1
docker run --rm --network none --label oqto.proof=pi1-cpu --ulimit core=0 \
  --user 65534:65534 --cap-drop ALL --security-opt no-new-privileges \
  --read-only --tmpfs /private:rw,mode=1777 --tmpfs /tmp:rw,exec,mode=1777 \
  -v "$emulator:/emulator:ro" -v "$runtime:/runtime:ro" \
  ubuntu@sha256:b1066385161d28ddf6bc7e7b28a9170eec11484c821d1a5150d176cbde41d7f7 \
  bash -lc 'set -e; export HOME=/private TMPDIR=/tmp; timeout -k 2 15 /emulator -cpu "$1" /runtime/pi --version' \
  -- "$cpu"
