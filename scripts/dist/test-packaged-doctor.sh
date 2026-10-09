#!/usr/bin/env bash
# Actual packaged doctor/activation runtime gates; NOT service readiness.
set -euo pipefail
artifact="$(realpath "${1:?candidate archive}")"
expected="$(awk '{print $1}' "$artifact.sha256")"
export DOCKER_HOST="${DOCKER_HOST:-unix:///var/run/docker.sock}"
docker run --rm -i --network none --user 65534:65534 --cap-drop ALL \
  --security-opt no-new-privileges --read-only --tmpfs /private:rw,exec,mode=1777 \
  -e EXPECTED_ARTIFACT_SHA256="$expected" -v "$(dirname "$artifact"):/artifacts:ro" \
  ubuntu@sha256:a853f94d226358a79c740cfc7bce0c289748f3fe3488d921d038ccd752c61b60 \
  bash -s -- "$(basename "$artifact")" <<'CONTAINER'
set -euo pipefail
archive="/artifacts/$1"
test "$(sha256sum "$archive" | cut -d' ' -f1)" = "$EXPECTED_ARTIFACT_SHA256"
mkdir -p /private/{home,config,data,state,extract}
export HOME=/private/home XDG_CONFIG_HOME=/private/config XDG_DATA_HOME=/private/data XDG_STATE_HOME=/private/state
mkdir -p "$XDG_CONFIG_HOME/oqto"
printf '' > "$XDG_CONFIG_HOME/oqto/config.toml"
tar -xzf "$archive" -C /private/extract
release="/private/extract/${1%.tar.gz}"
cli="$release/immutable/bin/oqtoctl"
node="$release/immutable/pi-runtime/pi/node/bin/node"
"$release/immutable/bin/oqto-setup" check-release --release-dir "$release"
# Record all runtime proofs before failing the service-readiness regression.
false_acceptance=false
if "$cli" --json doctor --contract --profile personal --strict > /private/doctor-valid.json 2>/private/doctor-valid.err; then
  echo 'FAIL strict doctor falsely accepted an unprovisioned service host' >&2
  false_acceptance=true
fi
"$node" -e 'const f=JSON.parse(require("fs").readFileSync(process.argv[1])).findings; if(f.some(x=>x.id==="release.runtime-or-private-state"))process.exit(1); console.log("PASS valid packaged runtime, strict service readiness remains blocked:", f.map(x=>x.id).join(", "))' /private/doctor-valid.json
receipt="$release/immutable/pi-runtime/pi/oqto-runtime.json"
cp "$receipt" /private/receipt-backup.json
printf '{"kind":"unsupported"}\n' > "$receipt"
if "$release/immutable/bin/oqto-setup" check-release --release-dir "$release" > /private/rejected-runtime.log 2>&1; then
  echo 'FAIL activation accepted corrupted receipt' >&2; exit 1
fi
if "$cli" --json doctor --contract --profile personal --strict > /private/doctor-invalid.json 2>/private/doctor-invalid.err; then
  echo 'FAIL doctor accepted corrupted receipt' >&2; exit 1
fi
"$node" -e 'const f=JSON.parse(require("fs").readFileSync(process.argv[1])).findings; if(!f.some(x=>x.id==="release.runtime-or-private-state"))process.exit(1); console.log("PASS actual packaged doctor and activation reject corrupted runtime")' /private/doctor-invalid.json
cp /private/receipt-backup.json "$receipt"
shard="$HOME/.local/share/oqto/oqto-log/example"
mkdir -p "$shard"
printf 'metadata-only DB fixture\n' > "$shard/oqto-log.sqlite"
ln -s ../unexpected "$shard/oqto-log.sqlite-wal"
before="$(sha256sum "$shard/oqto-log.sqlite")"
if "$cli" --json doctor --contract --profile personal --strict > /private/doctor-owner.json 2>/private/doctor-owner.err; then
  echo 'FAIL doctor accepted nonregular history sidecar' >&2; exit 1
fi
"$node" -e 'const f=JSON.parse(require("fs").readFileSync(process.argv[1])).findings; if(!f.some(x=>x.id==="release.runtime-or-private-state"))process.exit(1); console.log("PASS actual packaged doctor rejects nonregular WAL by metadata")' /private/doctor-owner.json
test "$(sha256sum "$shard/oqto-log.sqlite")" = "$before"
test -L "$shard/oqto-log.sqlite-wal"
test ! -e "$shard/oqto-log.sqlite-shm"
echo 'PASS metadata audit preserves DB/sidecar state; full service doctor acceptance NOT claimed'
if "$false_acceptance"; then
  "$node" -e 'console.log(JSON.stringify(JSON.parse(require("fs").readFileSync(process.argv[1])),null,2))' /private/doctor-valid.json
  exit 1
fi
CONTAINER
