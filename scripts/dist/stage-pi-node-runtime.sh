#!/usr/bin/env bash
# Prepare an explicit NEW release-owned Node/npm Pi tree. No live promotion.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
[[ $# == 1 ]] || { echo 'Usage: stage-pi-node-runtime.sh NEW_OUTPUT_DIRECTORY' >&2; exit 2; }
[[ $(uname -s)-$(uname -m) == Linux-x86_64 ]] || { echo 'Only Linux x86_64 is qualified' >&2; exit 2; }
output="$(realpath -m -- "$1")"
[[ ! -e "$output" && ! -L "$output" ]] || { echo 'Refusing to replace an existing runtime' >&2; exit 1; }
readarray -t pin < <(python3 - "$ROOT" <<'PY'
import json, pathlib, re, sys, tomllib
root = pathlib.Path(sys.argv[1])
p = tomllib.loads((root / 'dependencies.toml').read_text())['external']
lock = json.loads((root / 'scripts/dist/pi-node/package-lock.json').read_text())
package = json.loads((root / 'scripts/dist/pi-node/package.json').read_text())
sdk = lock['packages']['node_modules/@earendil-works/pi-coding-agent']
assert re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', p['pi'])
assert p['pi'] == sdk['version'] == package['dependencies']['@earendil-works/pi-coding-agent']
assert lock['packages']['']['dependencies'] == package['dependencies']
assert p['node'] == '24.14.1'
assert len(p['node-linux-x64-sha256']) == 64
for name, entry in lock['packages'].items():
    if name:
        assert entry['resolved'].startswith('https://registry.npmjs.org/'), name
        assert entry['integrity'].startswith('sha512-'), name
print(p['pi']); print(p['node']); print(p['node-linux-x64-sha256']); print(sdk['integrity'])
PY
)
[[ ${#pin[@]} == 4 ]] || { echo 'Invalid/mismatched runtime pins or lock' >&2; exit 1; }
mkdir -p "$(dirname "$output")"
build="$(mktemp -d "$(dirname "$output")/.pi-node-build.XXXXXX")"
trap 'rm -rf "$build"' EXIT
archive="node-v${pin[1]}-linux-x64.tar.xz"
if [[ -n ${PI_NODE_SOURCE_ARCHIVE:-} ]]; then
  cp -- "$PI_NODE_SOURCE_ARCHIVE" "$build/$archive"
else
  curl --fail --location --silent --show-error --retry 3 "https://nodejs.org/dist/v${pin[1]}/$archive" -o "$build/$archive"
fi
printf '%s  %s\n' "${pin[2]}" "$build/$archive" | sha256sum --check
mkdir "$build/payload"
tar -xJf "$build/$archive" -C "$build/payload"
mv "$build/payload/node-v${pin[1]}-linux-x64" "$build/payload/node"
cp "$ROOT/scripts/dist/pi-node/"package{,-lock}.json "$build/payload/"
mkdir "$build/home"
touch "$build/home/npmrc" "$build/home/globalnpmrc"
# No dependency lifecycle scripts or inherited credentials/npm config. npm ci
# verifies each locked integrity and refuses a package/lock disagreement.
(
  cd "$build/payload"
  env -i HOME="$build/home" PATH="$build/payload/node/bin:/usr/bin:/bin" \
    NPM_CONFIG_CACHE="$build/cache" NPM_CONFIG_UPDATE_NOTIFIER=false \
    "$build/payload/node/bin/node" "$build/payload/node/lib/node_modules/npm/bin/npm-cli.js" \
    ci --ignore-scripts --omit=dev --no-audit --no-fund \
    --userconfig "$build/home/npmrc" --globalconfig "$build/home/globalnpmrc"
)
cp "$ROOT/scripts/dist/pi-node/pi" "$build/payload/pi"
chmod 755 "$build/payload/pi"
[[ $("$build/payload/node/bin/node" --version) == "v${pin[1]}" ]]
[[ $(env -i HOME="$build/home" PATH=/usr/bin:/bin "$build/payload/pi" --version) == "${pin[0]}" ]]
python3 - "$build/payload" "${pin[0]}" "${pin[1]}" "${pin[2]}" "${pin[3]}" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
sdk = root / 'node_modules/@earendil-works/pi-coding-agent'
assert json.loads((sdk / 'package.json').read_text())['version'] == sys.argv[2]
for theme in ('dark.json', 'light.json'):
    assert (sdk / 'dist/modes/interactive/theme' / theme).is_file()
(root / 'oqto-runtime.json').write_text(json.dumps({
    'kind': 'node-npm', 'version': sys.argv[2], 'node_version': sys.argv[3],
    'node_archive_sha256': sys.argv[4], 'pi_npm_integrity': sys.argv[5],
    'package_lock_sha256': hashlib.sha256((root / 'package-lock.json').read_bytes()).hexdigest(),
}, indent=2) + '\n')
PY
# Fails without overwriting a destination which appeared during preparation.
mv -T --no-clobber "$build/payload" "$output"
[[ ! -d "$build/payload" ]] || { echo 'Destination appeared; refused promotion' >&2; exit 1; }
echo "Staged exact Pi ${pin[0]} with pinned Node ${pin[1]}: $output (no live activation)"
