#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP="$(mktemp -d)"
RUNNER_PID=""
trap '[[ -z "$RUNNER_PID" ]] || kill "$RUNNER_PID" 2>/dev/null; rm -rf "$TMP"' EXIT
# Never consult the developer's real user services: the installer refuses when
# the live runner is release-owned. A stub reports a runner PID only on request.
mkdir -p "$TMP/fakebin"
cat >"$TMP/fakebin/systemctl" <<'SH'
#!/usr/bin/env bash
cat "${FAKE_RUNNER_PID_FILE:-/dev/null}" 2>/dev/null || echo 0
SH
chmod 755 "$TMP/fakebin/systemctl"
export PATH="$TMP/fakebin:$PATH"
# Likewise never read the developer's real Oqto/Pi configuration.
export XDG_CONFIG_HOME="$TMP/home/.config"
VERSION=1.2.3
RELEASE="$TMP/releases/v$VERSION"
mkdir -p "$RELEASE/payload/pi" "$TMP/home/.pi/agent"
# Binary verification is hermetic (`-ne`), so package-provided providers listed
# by the invoking user must not be required in the resulting model catalog.
printf '{"packages":["npm:pi-claude-bridge"]}\n' >"$TMP/home/.pi/agent/settings.json"

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
mkdir -p "$RELEASE/payload/pi/theme"
printf '{}\n' >"$RELEASE/payload/pi/theme/dark.json"
printf '{}\n' >"$RELEASE/payload/pi/theme/light.json"
tar -czf "$RELEASE/pi-linux-x64.tar.gz" -C "$RELEASE/payload" pi
SHA="$(sha256sum "$RELEASE/pi-linux-x64.tar.gz" | awk '{print $1}')"
cp "$RELEASE/pi-linux-x64.tar.gz" "$TMP/valid-archive.tar.gz"

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
cp -R "$RELEASE/payload/pi/theme" "$TMP/releases/v1.2.2/payload/pi/theme"
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

# A version/model-discovery smoke cannot authorize an automatic major bump.
# Both promotion surfaces fail closed before acquisition or mutation, and the
# pin plus existing user runtime remain byte-for-byte/target-for-target intact.
python3 - "$TMP/release.json" "$TMP/major-release.json" <<'PY'
import json, sys
release = json.load(open(sys.argv[1], encoding="utf-8"))
release["tag_name"] = "v2.0.0"
with open(sys.argv[2], "w", encoding="utf-8") as output:
    json.dump(release, output)
PY
before_manifest="$(sha256sum "$TMP/dependencies.toml")"
before_current="$(readlink "$USER_RUNTIME")"
for mode in --update-lock --install-user; do
  if HOME="$TMP/home" XDG_DATA_HOME="$TMP/home/.local/share" \
    PI_RELEASE_API_URL="file://$TMP/major-release.json" \
    PI_RELEASE_BASE_URL="file://$TMP/releases" \
    "$SCRIPT_DIR/check-agent-runtime.sh" --manifest "$TMP/dependencies.toml" "$mode" \
    >"$TMP/major.out" 2>&1; then
    echo "test-pi-runtime: unqualified major promotion unexpectedly passed ($mode)" >&2
    exit 1
  fi
  grep -q 'major-version promotion requires reviewed integration qualification' "$TMP/major.out"
  [[ "$(sha256sum "$TMP/dependencies.toml")" == "$before_manifest" ]]
  [[ "$(readlink "$USER_RUNTIME")" == "$before_current" ]]
done

# Build output in a checkout (just dist-stage-pi-runtime) must not change the
# download source: the default official URL is still used and verified.
CHECKOUT="$TMP/checkout"
mkdir -p "$CHECKOUT/scripts/dist" "$CHECKOUT/dist/immutable/pi-runtime/pi" "$TMP/curlbin" "$TMP/staged-home"
cp "$SCRIPT_DIR/pi-runtime.sh" "$CHECKOUT/scripts/dist/"
cat >"$TMP/curlbin/curl" <<SH
#!/usr/bin/env bash
out=""; url=""
while [[ \$# -gt 0 ]]; do
  case "\$1" in -o) out="\$2"; shift 2 ;; http*) url="\$1"; shift ;; *) shift ;; esac
done
echo "\$url" >> "$TMP/curl-urls"
cp "$TMP/valid-archive.tar.gz" "\$out"
SH
chmod 755 "$TMP/curlbin/curl"
env -u PI_RELEASE_BASE_URL PATH="$TMP/curlbin:$PATH" HOME="$TMP/staged-home" \
  XDG_DATA_HOME="$TMP/staged-home/.local/share" \
  "$CHECKOUT/scripts/dist/pi-runtime.sh" --version "$VERSION" --sha256 "$SHA" --install-user >/dev/null
grep -Fxq "https://github.com/earendil-works/pi/releases/download/v$VERSION/pi-linux-x64.tar.gz" "$TMP/curl-urls"
[[ -x "$TMP/staged-home/.local/share/oqto/pi-runtimes/current/pi" ]]

# A release-installed runner only resolves its bundled Pi: --install-user must
# refuse before acquisition and leave the user channel untouched.
mkdir -p "$TMP/rel/immutable/bin" "$TMP/rel/immutable/pi-runtime/pi"
cp "$(command -v sleep)" "$TMP/rel/immutable/bin/oqto-runner"
printf '#!/usr/bin/env bash\necho 9.9.9\n' > "$TMP/rel/immutable/pi-runtime/pi/pi"
chmod 755 "$TMP/rel/immutable/pi-runtime/pi/pi"
"$TMP/rel/immutable/bin/oqto-runner" 300 & RUNNER_PID=$!
echo "$RUNNER_PID" > "$TMP/runner-pid"
before_current="$(readlink "$USER_RUNTIME")"
rm -f "$TMP/curl-urls"
if FAKE_RUNNER_PID_FILE="$TMP/runner-pid" PATH="$TMP/curlbin:$PATH" \
  HOME="$TMP/home" XDG_DATA_HOME="$TMP/home/.local/share" \
  "$SCRIPT_DIR/check-agent-runtime.sh" --manifest "$TMP/dependencies.toml" --install-user \
  >"$TMP/release-owned.out" 2>&1; then
  echo "test-pi-runtime: install into unused user channel unexpectedly passed" >&2
  exit 1
fi
grep -q 'uses its release-bundled Pi' "$TMP/release-owned.out"
grep -q '9.9.9' "$TMP/release-owned.out"
[[ ! -e "$TMP/curl-urls" ]] || { echo "test-pi-runtime: refusal downloaded first" >&2; exit 1; }
[[ "$(readlink "$USER_RUNTIME")" == "$before_current" ]]

# A custom executable means the runner does consult it: no refusal then.
mkdir -p "$TMP/home/.config/oqto"
printf '[pi]\nexecutable = "/opt/custom/pi"\n' > "$TMP/home/.config/oqto/config.toml"
FAKE_RUNNER_PID_FILE="$TMP/runner-pid" HOME="$TMP/home" XDG_DATA_HOME="$TMP/home/.local/share" \
  PI_RELEASE_API_URL="file://$TMP/release.json" PI_RELEASE_BASE_URL="file://$TMP/releases" \
  "$SCRIPT_DIR/check-agent-runtime.sh" --manifest "$TMP/dependencies.toml" --install-user >/dev/null

echo "test-pi-runtime: pass"
