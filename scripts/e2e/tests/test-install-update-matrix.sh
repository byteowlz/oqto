#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
matrix="$root/scripts/e2e/install-update-matrix.sh"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT

case "$(uname -s):$(uname -m)" in
  Linux:x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux:aarch64) target=aarch64-unknown-linux-gnu ;;
  *) echo 'skipped: Linux target required for install matrix preflight'; exit 0 ;;
esac

name="oqto-v9.8.7-${target}.tar.gz"
staging="${name%.tar.gz}"
mkdir -p "$scratch/bundle/$staging/immutable/bin"
printf 'manifest_version = 1\n' > "$scratch/bundle/$staging/manifest.toml"
mkdir -p "$scratch/bundle/$staging/immutable/frontend/dist"
printf '<main>Oqto</main>\n' > "$scratch/bundle/$staging/immutable/frontend/dist/index.html"
for bin in oqto oqtoctl oqto-setup oqto-runner oqto-files oqto-sandbox oqto-usermgr pi-bridge; do
  cp /usr/bin/true "$scratch/bundle/$staging/immutable/bin/$bin"
  chmod 0755 "$scratch/bundle/$staging/immutable/bin/$bin"
done
tar -C "$scratch/bundle" -czf "$scratch/$name" "$staging"
(cd "$scratch" && sha256sum "$name" > artifact.sha256)

scenario=fresh
if [[ -e /var/lib/oqto/releases/current || -L /var/lib/oqto/releases/current ]]; then
  scenario=upgrade
fi

pass() { printf 'PASS: %s\n' "$1"; }
rejects() {
  local label="$1" needle="$2"
  shift 2
  if "$matrix" "$@" > "$scratch/output" 2>&1; then
    echo "FAIL: $label unexpectedly passed" >&2
    exit 1
  fi
  if ! grep -Fq "$needle" "$scratch/output"; then
    echo "FAIL: $label returned the wrong diagnostic" >&2
    /usr/bin/tail -12 "$scratch/output" >&2
    exit 1
  fi
  pass "$label"
}

"$matrix" --profiles 'personal team' > "$scratch/plan"
grep -Fq 'read-only plan' "$scratch/plan"
grep -Fq 'no source build' "$scratch/plan"
pass 'default plan does not install or build'

args=(--scenario "$scenario" --profiles personal
  --artifact "$scratch/$name" --checksum "$scratch/artifact.sha256")
"$matrix" --preflight "${args[@]}" > "$scratch/positive"
grep -Fq 'read-only preflight passed' "$scratch/positive"
grep -Fq 'recovery=not-provided' "$scratch/positive"
pass 'matching, checksummed bundle is preflighted without claiming a VM snapshot'

printf 'placeholder\n' > "$scratch/bundle/$staging/immutable/bin/.gitkeep"
tar -C "$scratch/bundle" -czf "$scratch/$name" "$staging"
(cd "$scratch" && sha256sum "$name" > artifact.sha256)
rejects 'placeholder shipped as executable entrypoint' 'canonical bundle must not ship hidden bin entries' \
  --preflight --scenario "$scenario" --profiles personal \
  --artifact "$scratch/$name" --checksum "$scratch/artifact.sha256"
rm "$scratch/bundle/$staging/immutable/bin/.gitkeep"
tar -C "$scratch/bundle" -czf "$scratch/$name" "$staging"
(cd "$scratch" && sha256sum "$name" > artifact.sha256)

rm "$scratch/bundle/$staging/immutable/frontend/dist/index.html"
tar -C "$scratch/bundle" -czf "$scratch/$name" "$staging"
(cd "$scratch" && sha256sum "$name" > artifact.sha256)
rejects 'full bundle missing frontend index' 'canonical release bundle must contain exactly one' \
  --preflight --scenario "$scenario" --profiles personal \
  --artifact "$scratch/$name" --checksum "$scratch/artifact.sha256"
printf '<main>Oqto</main>\n' > "$scratch/bundle/$staging/immutable/frontend/dist/index.html"
tar -C "$scratch/bundle" -czf "$scratch/$name" "$staging"
(cd "$scratch" && sha256sum "$name" > artifact.sha256)

mkdir -p "$scratch/quoted path's"
cp "$scratch/$name" "$scratch/quoted path's/$name"
(cd "$scratch/quoted path's" && sha256sum "$name" > artifact.sha256)
"$matrix" --preflight --scenario "$scenario" --snapshot-id isolated-vm-before-install \
  --profiles personal --artifact "$scratch/quoted path's/$name" \
  --checksum "$scratch/quoted path's/artifact.sha256" > "$scratch/quoted-path-result"
grep -Fq 'read-only preflight passed' "$scratch/quoted-path-result"
pass 'artifact path with whitespace and shell quotes is handled literally'

mkdir -p "$scratch/legacy/$staging/bin"
cp /usr/bin/true "$scratch/legacy/$staging/bin/oqto-setup"
tar -C "$scratch/legacy" -czf "$scratch/legacy/$name" "$staging"
(cd "$scratch/legacy" && sha256sum "$name" > artifact.sha256)
rejects 'old flat-bin release layout' 'canonical release bundle must contain exactly one' \
  --preflight --scenario "$scenario" --profiles personal \
  --artifact "$scratch/legacy/$name" --checksum "$scratch/legacy/artifact.sha256"

rejects 'missing recovery choice for execute' 'execute requires --snapshot-id or --disposable-vm' --execute "${args[@]}"
test_host="$(uname -n)"
test_host="${test_host%%.*}"
"$matrix" --preflight --disposable-vm "$test_host" "${args[@]}" > "$scratch/disposable"
grep -Fq "recovery=disposable:$test_host" "$scratch/disposable"
pass 'explicit hostname-matched throwaway VM needs no snapshot for read-only preflight'
rejects 'mismatched disposable hostname' 'disposable VM hostname mismatch' \
  --execute --disposable-vm wrong-test-host "${args[@]}"
rejects 'conflicting recovery choices' 'either --snapshot-id or --disposable-vm' \
  --execute --snapshot-id test-snapshot --disposable-vm "$test_host" "${args[@]}"
rejects 'two profiles on one host' 'one profile per isolated VM' \
  --preflight --scenario "$scenario" --snapshot-id isolated-vm-before-install \
  --profiles 'personal team' --artifact "$scratch/$name" --checksum "$scratch/artifact.sha256"
rejects 'source-build default forbidden on execute' 'provide a release --artifact' \
  --execute --scenario "$scenario" --snapshot-id isolated-vm-before-install --profiles personal

other_scenario=upgrade
host_state_error='upgrade test requires an existing active release'
if [[ "$scenario" == upgrade ]]; then
  other_scenario=fresh
  host_state_error='fresh-install test refused'
fi
rejects 'wrong fresh/upgrade host state' "$host_state_error" \
  --preflight --scenario "$other_scenario" --snapshot-id isolated-vm-before-install \
  --profiles personal --artifact "$scratch/$name" --checksum "$scratch/artifact.sha256"

wrong_name="oqto-v9.8.7-aarch64-unknown-linux-gnu.tar.gz"
[[ "$target" == aarch64-unknown-linux-gnu ]] && wrong_name='oqto-v9.8.7-x86_64-unknown-linux-gnu.tar.gz'
cp "$scratch/$name" "$scratch/$wrong_name"
(cd "$scratch" && sha256sum "$wrong_name" > wrong.sha256)
rejects 'other-architecture bundle' 'does not match host target' \
  --preflight --scenario "$scenario" --snapshot-id isolated-vm-before-install \
  --profiles personal --artifact "$scratch/$wrong_name" --checksum "$scratch/wrong.sha256"

printf '%064d  %s\n' 0 "$name" > "$scratch/bad.sha256"
rejects 'wrong content hash' 'release artifact checksum mismatch' \
  --preflight --scenario "$scenario" --snapshot-id isolated-vm-before-install \
  --profiles personal --artifact "$scratch/$name" --checksum "$scratch/bad.sha256"

cp "$scratch/artifact.sha256" "$scratch/combined.txt"
printf '%064d  other.tar.gz\n' 0 >> "$scratch/combined.txt"
rejects 'ambiguous combined checksum' 'one per-artifact checksum line' \
  --preflight --scenario "$scenario" --snapshot-id isolated-vm-before-install \
  --profiles personal --artifact "$scratch/$name" --checksum "$scratch/combined.txt"

printf '%s  unrelated.tar.gz\n' "$(sha256sum "$scratch/$name" | cut -d' ' -f1)" > "$scratch/wrong-name.sha256"
rejects 'checksum references another bundle' 'checksum filename does not match artifact' \
  --preflight --scenario "$scenario" --snapshot-id isolated-vm-before-install \
  --profiles personal --artifact "$scratch/$name" --checksum "$scratch/wrong-name.sha256"

mkdir "$scratch/mockbin"
printf '#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo arm64;; esac\n' > "$scratch/mockbin/uname"
chmod +x "$scratch/mockbin/uname"
if PATH="$scratch/mockbin:$PATH" "$matrix" --preflight "${args[@]}" > "$scratch/output" 2>&1; then
  echo 'FAIL: macOS activation unexpectedly permitted' >&2
  exit 1
fi
grep -Fq 'macOS activation is not implemented' "$scratch/output"
pass 'macOS activation fails closed until a native installer exists'

rm "$scratch/bundle/$staging/immutable/bin/oqto-files"
tar -C "$scratch/bundle" -czf "$scratch/$name" "$staging"
(cd "$scratch" && sha256sum "$name" > artifact.sha256)
rejects 'bundle missing required binary' 'canonical release bundle must contain exactly one' \
  --preflight --scenario "$scenario" --snapshot-id isolated-vm-before-install \
  --profiles personal --artifact "$scratch/$name" --checksum "$scratch/artifact.sha256"
cp /usr/bin/true "$scratch/bundle/$staging/immutable/bin/oqto-files"
printf 'GLIBC_999.0\n' >> "$scratch/bundle/$staging/immutable/bin/oqto-setup"
tar -C "$scratch/bundle" -czf "$scratch/$name" "$staging"
(cd "$scratch" && sha256sum "$name" > artifact.sha256)
rejects 'newer-glibc embedded installer' 'requires GLIBC_999.0' \
  --preflight --scenario "$scenario" --snapshot-id isolated-vm-before-install \
  --profiles personal --artifact "$scratch/$name" --checksum "$scratch/artifact.sha256"

rm "$scratch/bundle/$staging/immutable/bin/oqto-setup"
ln -s /etc/passwd "$scratch/bundle/$staging/immutable/bin/oqto-setup"
tar -C "$scratch/bundle" -czf "$scratch/$name" "$staging"
(cd "$scratch" && sha256sum "$name" > artifact.sha256)
rejects 'embedded installer symlink' 'embedded oqto-setup must be a regular file' \
  --preflight --scenario "$scenario" --snapshot-id isolated-vm-before-install \
  --profiles personal --artifact "$scratch/$name" --checksum "$scratch/artifact.sha256"

printf 'test\n' > "$scratch/not-a-bundle"
cp "$scratch/not-a-bundle" "$scratch/$name"
(cd "$scratch" && sha256sum "$name" > artifact.sha256)
rejects 'verified bytes without installer member' 'verified bundle is not a valid gzip tar archive' \
  --preflight --scenario "$scenario" --snapshot-id isolated-vm-before-install \
  --profiles personal --artifact "$scratch/$name" --checksum "$scratch/artifact.sha256"

echo 'Install/update matrix read-only safety tests passed.'
