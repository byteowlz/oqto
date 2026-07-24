#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
VERSION=1.2.3
RELEASE="$TMP/releases/v$VERSION"
mkdir -p "$RELEASE/payload/pi" "$TMP/home"

cat >"$RELEASE/payload/pi/pi" <<'SH'
#!/usr/bin/env bash
if [[ "${1:-}" == "--version" ]]; then
  echo 1.2.3
  exit 0
fi
if [[ " $* " == *" --mode rpc "* ]]; then
  while IFS= read -r line; do
    [[ "$line" == *'oqto-pi-runtime-smoke'* ]] || continue
    printf '%s\n' '{"id":"oqto-pi-runtime-smoke","type":"response","success":true,"data":{"models":[{"provider":"fixture","id":"fixture"}]}}'
  done
  exit 0
fi
exit 2
SH
chmod 755 "$RELEASE/payload/pi/pi"
mkdir -p "$RELEASE/payload/pi/theme"
printf '{}\n' >"$RELEASE/payload/pi/theme/dark.json"
printf '{}\n' >"$RELEASE/payload/pi/theme/light.json"
tar -czf "$RELEASE/pi-linux-x64.tar.gz" -C "$RELEASE/payload" pi
SHA="$(sha256sum "$RELEASE/pi-linux-x64.tar.gz" | awk '{print $1}')"

# RPC mode does not load TUI assets. A checksum-valid archive without a built-in
# theme must still fail before it can be promoted.
mkdir -p "$TMP/incomplete/pi/theme" "$TMP/incomplete-release/v$VERSION"
cp "$RELEASE/payload/pi/pi" "$TMP/incomplete/pi/pi"
cp "$RELEASE/payload/pi/theme/light.json" "$TMP/incomplete/pi/theme/light.json"
tar -czf "$TMP/incomplete-release/v$VERSION/pi-linux-x64.tar.gz" -C "$TMP/incomplete" pi
INCOMPLETE_SHA="$(sha256sum "$TMP/incomplete-release/v$VERSION/pi-linux-x64.tar.gz" | awk '{print $1}')"
if HOME="$TMP/home" XDG_DATA_HOME="$TMP/home/.local/share" \
  "$SCRIPT_DIR/pi-runtime.sh" \
  --version "$VERSION" \
  --sha256 "$INCOMPLETE_SHA" \
  --base-url "file://$TMP/incomplete-release" \
  >"$TMP/incomplete.out" 2>&1; then
  echo "test-pi-runtime: archive missing dark theme unexpectedly passed" >&2
  exit 1
fi
grep -q 'missing required asset: theme/dark.json' "$TMP/incomplete.out"

HOME="$TMP/home" XDG_DATA_HOME="$TMP/home/.local/share" \
  "$SCRIPT_DIR/pi-runtime.sh" \
  --version "$VERSION" \
  --sha256 "$SHA" \
  --base-url "file://$TMP/releases" \
  >/dev/null

HOME="$TMP/home" XDG_DATA_HOME="$TMP/home/.local/share" \
  "$SCRIPT_DIR/pi-runtime.sh" \
  --version "$VERSION" \
  --sha256 "$SHA" \
  --base-url "file://$TMP/releases" \
  --install-user >/dev/null
USER_RUNTIME="$TMP/home/.local/share/oqto/pi-runtimes/current"
[[ "$("$USER_RUNTIME/pi" --version)" == "$VERSION" ]]
grep -q "\"archive_sha256\":\"$SHA\"" "$USER_RUNTIME/oqto-runtime.json"

wrong_sha="${SHA%?}0"
[[ "$wrong_sha" != "$SHA" ]] || wrong_sha="${SHA%?}1"
if HOME="$TMP/home" XDG_DATA_HOME="$TMP/home/.local/share" \
  "$SCRIPT_DIR/pi-runtime.sh" \
  --version "$VERSION" \
  --sha256 "$wrong_sha" \
  --base-url "file://$TMP/releases" \
  >/dev/null 2>&1; then
  echo "test-pi-runtime: checksum mismatch unexpectedly passed" >&2
  exit 1
fi

# Exercise the deterministic update-lock path against a local release API.
cp "$RELEASE/pi-linux-x64.tar.gz" "$RELEASE/pi-linux-arm64.tar.gz"
ARM_SHA="$(sha256sum "$RELEASE/pi-linux-arm64.tar.gz" | awk '{print $1}')"
printf '%s  %s\n%s  %s\n' \
  "$SHA" pi-linux-x64.tar.gz \
  "$ARM_SHA" pi-linux-arm64.tar.gz >"$RELEASE/SHA256SUMS"
cat >"$TMP/release.json" <<JSON
{
  "tag_name": "v$VERSION",
  "assets": [{
    "name": "SHA256SUMS",
    "browser_download_url": "file://$RELEASE/SHA256SUMS"
  }]
}
JSON
cat >"$TMP/dependencies.toml" <<'TOML'
[external]
pi = "1.2.2"
pi-linux-x64-sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
pi-linux-arm64-sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
TOML
HOME="$TMP/home" \
XDG_DATA_HOME="$TMP/home/.local/share" \
PI_RELEASE_API_URL="file://$TMP/release.json" \
PI_RELEASE_BASE_URL="file://$TMP/releases" \
  "$SCRIPT_DIR/check-agent-runtime.sh" \
    --manifest "$TMP/dependencies.toml" --update-lock >/dev/null

grep -q '^pi = "1.2.3"' "$TMP/dependencies.toml"
grep -q "^pi-linux-x64-sha256 = \"$SHA\"" "$TMP/dependencies.toml"
grep -q "^pi-linux-arm64-sha256 = \"$ARM_SHA\"" "$TMP/dependencies.toml"

echo "test-pi-runtime: pass"
