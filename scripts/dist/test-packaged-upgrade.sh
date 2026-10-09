#!/usr/bin/env bash
# Two canonical locally built releases: activation, rollback and preservation.
# Not published-predecessor, service/profile or browser acceptance.
set -euo pipefail
candidate="$(realpath "${1:?candidate archive}")"
previous="$(realpath "${2:?canonical predecessor archive}")"
[[ -f "$candidate.sha256" && -f "$previous.sha256" ]]
export DOCKER_HOST="${DOCKER_HOST:-unix:///var/run/docker.sock}"
version="${EXPECTED_PI_VERSION:?set the exact candidate Pi version}"
docker run --rm -i --network none --user 65534:65534 --cap-drop ALL -e EXPECTED_PI_VERSION="$version" \
  --security-opt no-new-privileges --read-only --tmpfs /private:rw,exec,mode=1777 \
  -v "$(dirname "$candidate"):/candidate:ro" -v "$(dirname "$previous"):/previous:ro" \
  ubuntu@sha256:a853f94d226358a79c740cfc7bce0c289748f3fe3488d921d038ccd752c61b60 \
  bash -s -- "$(basename "$candidate")" "$(basename "$previous")" <<'CONTAINER'
set -euo pipefail
candidate="/candidate/$1"
previous="/previous/$2"
new_id="${1%.tar.gz}"
old_id="${2%.tar.gz}"
mkdir -p /private/{home,config,data,state,extract,bin}
export HOME=/private/home XDG_CONFIG_HOME=/private/config
export XDG_DATA_HOME=/private/data XDG_STATE_HOME=/private/state
export PI_CODING_AGENT_DIR=/private/home/.pi/agent
mkdir -p "$PI_CODING_AGENT_DIR" /private/config/oqto
printf '{"custom":{"keep":true},"extensions":["user-owned-plugin"]}\n' >"$PI_CODING_AGENT_DIR/settings.json"
printf '{"fixture_marker":"user-owned-auth-preserve","providers":{}}\n' >"$PI_CODING_AGENT_DIR/auth.json"
printf '# custom config\n[unknown]\nkeep = "unchanged"\n' >/private/config/oqto/config.toml
find /private/home /private/config -type f -print0 | sort -z | xargs -0 sha256sum >/private/before.sha256
# Inputs are trusted local build products, with hashes recorded separately.
for archive in "$candidate" "$previous"; do
  expected=$(awk '{print $1}' "$archive.sha256")
  test "$(sha256sum "$archive" | cut -d' ' -f1)" = "$expected"
done
tar -xzf "$previous" -C /private/extract "$old_id/immutable/bin/oqto-setup"
old_setup="/private/extract/$old_id/immutable/bin/oqto-setup"
"$old_setup" install --artifact "$previous" --checksum "$previous.sha256" \
  --releases-root /private/releases --bin-dir /private/bin --web-root /private/web \
  --doctor-strict false
/private/bin/oqto --version
old_current=$(readlink /private/releases/current)
old_good=$(readlink /private/releases/last-good)
old_entry=$(readlink /private/bin/oqto)
# Candidate installer is trusted locally built input, hash verified above.
tar -xzf "$candidate" -C /private/extract "$new_id/immutable/bin/oqto-setup"
new_setup="/private/extract/$new_id/immutable/bin/oqto-setup"
# Inject an invalid explicitly selected doctor config, not a fabricated doctor
# executable. The real staged oqtoctl must reject it after the pointer switch.
printf '[invalid syntax\n' >/private/doctor-invalid.toml
if OQTO_CONFIG=/private/doctor-invalid.toml "$new_setup" install --artifact "$candidate" --checksum "$candidate.sha256" \
  --releases-root /private/releases --bin-dir /private/bin --web-root /private/web \
  --doctor-strict true >/private/rejected.log 2>&1; then
  echo 'FAIL real staged doctor accepted invalid explicit config' >&2; exit 1
fi
cat /private/rejected.log
grep -q 'rolled back to /private/releases/' /private/rejected.log
test "$(readlink /private/releases/current)" = "$old_current"
test "$(readlink /private/releases/last-good)" = "$old_good"
test "$(readlink /private/bin/oqto)" = "$old_entry"
/private/bin/oqto --version
sha256sum --check /private/before.sha256
# Actual verified candidate upgrade, doctor delegated to future service acceptance.
"$new_setup" install --artifact "$candidate" --checksum "$candidate.sha256" \
  --releases-root /private/releases --bin-dir /private/bin --web-root /private/web \
  --doctor-strict false
test "$(basename "$(readlink /private/releases/current)")" = "$new_id"
test -f /private/web/index.html
test -x /private/bin/oqto-runner
test "$(/private/releases/current/immutable/pi-runtime/pi/pi --version)" = "$EXPECTED_PI_VERSION"
/private/bin/oqto --version
sha256sum --check /private/before.sha256
echo 'PASS upgrade preserves unknown config/auth/settings'
# Current policy refuses active-release replacement, including identical reinstall.
# Verify that refusal is non-destructive; do not describe it as a successful no-op.
if "$new_setup" install --artifact "$candidate" --checksum "$candidate.sha256" \
  --releases-root /private/releases --bin-dir /private/bin --web-root /private/web \
  --doctor-strict false >/private/reinstall.log 2>&1; then
  echo 'FAIL active release was unexpectedly overwritten' >&2; exit 1
fi
grep -q 'only strictly newer releases install' /private/reinstall.log
test "$(basename "$(readlink /private/releases/current)")" = "$new_id"
/private/bin/oqto --version
test -f /private/web/index.html
sha256sum --check /private/before.sha256
echo 'PASS identical reinstall refusal preserves active release and user config'
# Downgrade to the predecessor is refused before extraction, without mutation.
before_releases=$(ls /private/releases | sort | tr '\n' ' ')
if "$new_setup" install --artifact "$previous" --checksum "$previous.sha256" \
  --releases-root /private/releases --bin-dir /private/bin --web-root /private/web \
  --doctor-strict false >/private/downgrade.log 2>&1; then
  echo 'FAIL downgrade was installed without --allow-downgrade' >&2; exit 1
fi
cat /private/downgrade.log
grep -q 'only strictly newer releases install' /private/downgrade.log
test "$(basename "$(readlink /private/releases/current)")" = "$new_id"
test "$(ls /private/releases | sort | tr '\n' ' ')" = "$before_releases"
sha256sum --check /private/before.sha256
echo 'PASS downgrade refusal preserves active release, stages and user config'
echo 'PASS canonical development predecessor failed-activation restoration, upgrade, reinstall and downgrade refusal'
CONTAINER
