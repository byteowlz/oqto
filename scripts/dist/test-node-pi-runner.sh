#!/usr/bin/env bash
# Real release-path runner startup/resolution, NOT full activation or sandbox proof.
set -euo pipefail
[[ $# == 2 ]] || { echo 'Usage: test-node-pi-runner.sh RUNNER_BINARY NODE_RUNTIME' >&2; exit 2; }
runner="$(realpath "$1")"
runtime="$(realpath "$2")"
version="$(python3 - "$runtime/oqto-runtime.json" <<'PY'
import json, re, sys
version = json.load(open(sys.argv[1]))['version']
assert re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', version)
print(version)
PY
)"
export DOCKER_HOST="${DOCKER_HOST:-unix:///var/run/docker.sock}"
docker run --rm --network none --user 65534:65534 --cap-drop ALL -e EXPECTED_PI_VERSION="$version" \
  --security-opt no-new-privileges --read-only --tmpfs /private:rw,exec,mode=1777 \
  --tmpfs /tmp:rw,exec,mode=1777 -v "$runner:/runner-source:ro" \
  -v "$runtime:/runtime:ro" \
  ubuntu@sha256:b1066385161d28ddf6bc7e7b28a9170eec11484c821d1a5150d176cbde41d7f7 \
  bash -lc '
    set -euo pipefail
    export HOME=/private/home XDG_CONFIG_HOME=/private/config XDG_DATA_HOME=/private/data XDG_STATE_HOME=/private/state
    export PI_CODING_AGENT_DIR=/private/home/.pi/agent
    mkdir -p "$PI_CODING_AGENT_DIR" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME" /private/run /private/release/immutable/{bin,pi-runtime}
    cp /runner-source /private/release/immutable/bin/oqto-runner
    chmod 755 /private/release/immutable/bin/oqto-runner
    ln -s /runtime /private/release/immutable/pi-runtime/pi
    status=0
    timeout -k 3 8 /private/release/immutable/bin/oqto-runner --socket /private/run/runner.sock --no-sandbox -v >/private/runner.log 2>&1 || status=$?
    cat /private/runner.log
    test "$status" = 0 || test "$status" = 124
    grep -Fq "immutable/pi-runtime/pi/pi (v${EXPECTED_PI_VERSION})" /private/runner.log
    grep -q "Runner listening on unix:/private/run/runner.sock" /private/runner.log
    grep -q "Runner stopped" /private/runner.log
    test ! -S /private/run/runner.sock
    echo "PASS actual runner release-path Node Pi1 resolution, listener startup and shutdown"
  '
