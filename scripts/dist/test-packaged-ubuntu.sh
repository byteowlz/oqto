#!/usr/bin/env bash
# Disposable packaged activation + bundled-runtime startup, NOT full deployment acceptance.
set -euo pipefail
artifact="$(realpath "${1:?pass canonical Linux x86_64 release archive}")"
checksum="${artifact}.sha256"
[[ -f "$checksum" ]] || { echo 'missing checksum sidecar' >&2; exit 1; }
export DOCKER_HOST="${DOCKER_HOST:-unix:///var/run/docker.sock}"
version="${EXPECTED_PI_VERSION:?set the exact candidate Pi version}"
expected="$(awk '{print $1}' "$checksum")"
docker run --rm -i --network none --user 65534:65534 --cap-drop ALL \
  --security-opt no-new-privileges --read-only --tmpfs /private:rw,exec,mode=1777 --tmpfs /extract:rw,exec,mode=1777 \
  -e EXPECTED_PI_VERSION="$version" -e EXPECTED_ARTIFACT_SHA256="$expected" \
  -v "$(dirname "$artifact"):/artifacts:ro" \
  ubuntu@sha256:b1066385161d28ddf6bc7e7b28a9170eec11484c821d1a5150d176cbde41d7f7 \
  bash -s -- "$(basename "$artifact")" <<'CONTAINER'
set -euo pipefail
archive="/artifacts/$1"
test "$(sha256sum "$archive" | cut -d' ' -f1)" = "$EXPECTED_ARTIFACT_SHA256"
mkdir -p /private/{home,config,data,state}
export HOME=/private/home XDG_CONFIG_HOME=/private/config
export XDG_DATA_HOME=/private/data XDG_STATE_HOME=/private/state
export PI_CODING_AGENT_DIR=/private/home/.pi/agent
mkdir -p "$PI_CODING_AGENT_DIR"
tar -xzf "$archive" -C /extract
release="/extract/${1%.tar.gz}"
"$release/immutable/bin/oqto-setup" install --artifact "$archive" \
  --checksum "$archive.sha256" --releases-root /private/releases \
  --bin-dir /private/bin --web-root /private/web --doctor-strict false
test -f /private/web/index.html
runtime=/private/releases/current/immutable/pi-runtime/pi/pi
test "$("$runtime" --version)" = "$EXPECTED_PI_VERSION"
# Private fixture only: verifies runtime resolution, not sandbox acceptance.
timeout 5 /private/bin/oqto-runner --socket /private/runner.sock --no-sandbox -v \
  >/private/runner.log 2>&1 || test "$?" = 124
cat /private/runner.log
grep -Fq "immutable/pi-runtime/pi/pi (v${EXPECTED_PI_VERSION})" /private/runner.log
grep -q 'Runner listening on unix:/private/runner.sock' /private/runner.log
grep -q 'Runner stopped' /private/runner.log
test ! -S /private/runner.sock
ldd --version | head -1
echo 'PASS disposable Ubuntu22.04 activation + bundled Pi1 resolution/startup/shutdown'
CONTAINER
