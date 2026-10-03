#!/usr/bin/env bash
# Ship the exact official archive; installation reuses the verified runtime engine.
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
readarray -t pin < <(python3 - <<'PY'
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
with open('dependencies.toml', 'rb') as source:
    external = tomllib.load(source)['external']
print(external['pi'])
print(external['pi-linux-x64-sha256'])
PY
)
[[ ${pin[0]:-} == 1.0.0 && ${pin[1]:-} =~ ^[0-9a-f]{64}$ ]] || { echo 'error: release must ship pinned official Pi1.0.0' >&2; exit 1; }
destination="$ROOT_DIR/dist/immutable/pi-runtime/v${pin[0]}"
mkdir -p "$destination"
archive="$destination/pi-linux-x64.tar.gz"
if [[ -n "${PI_RUNTIME_SOURCE_ARCHIVE:-}" ]]; then
  cp "$PI_RUNTIME_SOURCE_ARCHIVE" "$archive"
else
  curl --fail --location --silent --show-error --retry 3 "https://github.com/earendil-works/pi/releases/download/v${pin[0]}/pi-linux-x64.tar.gz" -o "$archive"
fi
printf '%s  %s\n' "${pin[1]}" "$archive" | sha256sum --check
# Extract only the checksum-qualified official package into its immutable slot.
tar -xzf "$archive" -C "$ROOT_DIR/dist/immutable/pi-runtime"
scripts/dist/pi-runtime.sh --version "${pin[0]}" --sha256 "${pin[1]}"
echo "staged official Pi ${pin[0]} archive"
