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
  agent_dir="${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}"
  mkdir -p "$agent_dir"
  printf '{}\n' > "$agent_dir/auth.json"
  echo 1.2.3
  exit 0
fi
if [[ " $* " == *" --mode rpc "* ]]; then
  # Real Pi may create auth.json during RPC startup. A binary check must
  # never run in the invoking user's credential profile.
  agent_dir="${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}"
  mkdir -p "$agent_dir"
  printf '{}\n' > "$agent_dir/auth.json"
  while IFS= read -r line; do
    [[ "$line" == *'oqto-pi-runtime-smoke'* ]] || continue
    mode=available
    if [[ -f "$(dirname "$0")/fixture-mode" ]]; then
      IFS= read -r mode < "$(dirname "$0")/fixture-mode"
    fi
    case "$mode" in
      empty) printf '%s\n' '{"id":"oqto-pi-runtime-smoke","type":"response","success":true,"data":{"models":[]}}' ;;
      invalid) printf '%s\n' '{"id":"oqto-pi-runtime-smoke","type":"response","success":false,"error":"unsupported RPC"}' ;;
      *) printf '%s\n' '{"id":"oqto-pi-runtime-smoke","type":"response","success":true,"data":{"models":[{"provider":"fixture","id":"fixture"}]}}' ;;
    esac
  done
  exit 0
fi
exit 2
SH
chmod 755 "$RELEASE/payload/pi/pi"
tar -czf "$RELEASE/pi-linux-x64.tar.gz" -C "$RELEASE/payload" pi
SHA="$(sha256sum "$RELEASE/pi-linux-x64.tar.gz" | awk '{print $1}')"
cp "$RELEASE/pi-linux-x64.tar.gz" "$TMP/valid-archive.tar.gz"

mkdir -p "$TMP/home/.pi/agent"
printf 'preexisting user credential fixture\n' > "$TMP/home/.pi/agent/auth.json"
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
mkdir -p "$TMP/version-check-home"
[[ "$(HOME="$TMP/version-check-home" PI_CODING_AGENT_DIR="$TMP/version-check-home/agent" "$USER_RUNTIME/pi" --version)" == "$VERSION" ]]
grep -q "\"archive_sha256\":\"$SHA\"" "$USER_RUNTIME/oqto-runtime.json"
grep -Fxq 'preexisting user credential fixture' "$TMP/home/.pi/agent/auth.json" || {
  echo 'test-pi-runtime: binary verification modified user-owned auth.json' >&2
  exit 1
}

# A fresh user has not consented to a provider login yet. The binary must be
# installable before first-run OAuth; an empty successful RPC catalog is healthy
# binary transport, not proof that chat is ready.
mkdir -p "$TMP/empty-home"
printf 'empty\n' > "$RELEASE/payload/pi/fixture-mode"
tar -czf "$RELEASE/pi-linux-x64.tar.gz" -C "$RELEASE/payload" pi
EMPTY_SHA="$(sha256sum "$RELEASE/pi-linux-x64.tar.gz" | awk '{print $1}')"
HOME="$TMP/empty-home" XDG_DATA_HOME="$TMP/empty-home/.local/share" \
  "$SCRIPT_DIR/pi-runtime.sh" \
    --version "$VERSION" --sha256 "$EMPTY_SHA" \
    --base-url "file://$TMP/releases" --install-user >/dev/null
[[ -x "$TMP/empty-home/.local/share/oqto/pi-runtimes/current/pi" ]]
[[ ! -e "$TMP/empty-home/.pi/agent/auth.json" ]]

# A failed RPC call must still fail closed: don't equate an empty authenticated
# model list with a broken or unsupported runtime protocol.
printf 'invalid\n' > "$RELEASE/payload/pi/fixture-mode"
tar -czf "$RELEASE/pi-linux-x64.tar.gz" -C "$RELEASE/payload" pi
INVALID_SHA="$(sha256sum "$RELEASE/pi-linux-x64.tar.gz" | awk '{print $1}')"
if HOME="$TMP/empty-home" \
  "$SCRIPT_DIR/pi-runtime.sh" \
    --version "$VERSION" --sha256 "$INVALID_SHA" \
    --base-url "file://$TMP/releases" > "$TMP/invalid.log" 2>&1; then
  echo "test-pi-runtime: invalid RPC response unexpectedly passed" >&2
  exit 1
fi
grep -q 'RPC model discovery response failed' "$TMP/invalid.log"
cp "$TMP/valid-archive.tar.gz" "$RELEASE/pi-linux-x64.tar.gz"

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
# --verify-current must use the pinned version and SHA, NOT the newer version
# advertised by the release API. Use a distinct fixture to catch accidental
# promotion or misleading compatibility claims.
mkdir -p "$TMP/releases/v1.2.2/payload/pi"
cp "$RELEASE/payload/pi/pi" "$TMP/releases/v1.2.2/payload/pi/pi"
sed -i 's/1\.2\.3/1.2.2/g' "$TMP/releases/v1.2.2/payload/pi/pi"
tar -czf "$TMP/releases/v1.2.2/pi-linux-x64.tar.gz" \
  -C "$TMP/releases/v1.2.2/payload" pi
PINNED_SHA="$(sha256sum "$TMP/releases/v1.2.2/pi-linux-x64.tar.gz" | awk '{print $1}')"
sed -i "s/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/$PINNED_SHA/" "$TMP/dependencies.toml"
HOME="$TMP/home" PI_RELEASE_API_URL="file://$TMP/release.json" \
PI_RELEASE_BASE_URL="file://$TMP/releases" \
  "$SCRIPT_DIR/check-agent-runtime.sh" \
    --manifest "$TMP/dependencies.toml" --verify-current > "$TMP/pinned.log"
grep -Fq 'agent-check: pinned candidate 1.2.2 passed' "$TMP/pinned.log" || {
  echo 'test-pi-runtime: --verify-current tested latest rather than pinned release' >&2
  exit 1
}

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
