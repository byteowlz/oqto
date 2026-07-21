#!/usr/bin/env bash
# Deterministic Pi release checker/promoter. No LLM is involved.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
MANIFEST="$ROOT_DIR/dependencies.toml"
UPDATE_LOCK=false
VERIFY_CURRENT=false
API_URL="${PI_RELEASE_API_URL:-https://api.github.com/repos/earendil-works/pi/releases/latest}"

usage() {
  cat <<'EOF'
Usage: check-agent-runtime.sh [--manifest PATH] [--update-lock] [--verify-current]

Default: report whether a newer Pi release exists and compatibility-test it.
--update-lock updates Pi version and Linux checksums only after the test passes.
--verify-current tests the currently pinned release even when no update exists.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest) MANIFEST="$2"; shift 2 ;;
    --update-lock) UPDATE_LOCK=true; shift ;;
    --verify-current) VERIFY_CURRENT=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

read_pin() {
  local key="$1"
  sed -n '/^\[external\]/,/^\[/{s/^'"$key"' *= *"\([^"]*\)".*/\1/p}' "$MANIFEST" | head -1
}

CURRENT="$(read_pin pi)"
[[ -n "$CURRENT" ]] || { echo "agent-check: missing Pi pin in $MANIFEST" >&2; exit 1; }

headers=(-H 'Accept: application/vnd.github+json')
[[ -n "${GITHUB_TOKEN:-}" ]] && headers+=(-H "Authorization: Bearer $GITHUB_TOKEN")
release_json="$(mktemp)"; sums="$(mktemp)"; check_home="$(mktemp -d)"
trap 'rm -f "$release_json" "$sums"; rm -rf "$check_home"' EXIT
mkdir -p "$check_home/.pi/agent"
cat >"$check_home/.pi/agent/models.json" <<'JSON'
{
  "providers": {
    "oqto-runtime-smoke": {
      "baseUrl": "http://127.0.0.1:9/v1",
      "api": "openai-completions",
      "apiKey": "deterministic-offline-smoke",
      "models": [{
        "id": "runtime-smoke",
        "name": "Runtime Smoke",
        "contextWindow": 4096,
        "maxTokens": 256,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}
      }]
    }
  }
}
JSON
curl --fail --silent --show-error --location --retry 3 "${headers[@]}" "$API_URL" -o "$release_json"
LATEST="$(python3 - "$release_json" <<'PY'
import json, sys
release = json.load(open(sys.argv[1], encoding="utf-8"))
tag = str(release.get("tag_name", ""))
print(tag.removeprefix("v"))
PY
)"
[[ "$LATEST" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || {
  echo "agent-check: invalid latest release tag: ${LATEST:-<empty>}" >&2
  exit 1
}

echo "agent-check: pinned=$CURRENT latest=$LATEST"
if [[ "$LATEST" == "$CURRENT" && "$VERIFY_CURRENT" != "true" ]]; then
  echo "agent-check: pin is current"
  exit 0
fi
if [[ "$LATEST" != "$CURRENT" ]] && ! python3 - "$CURRENT" "$LATEST" <<'PY'
import re, sys

def key(value: str):
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:[-.]([0-9A-Za-z.-]+))?", value)
    if not match:
        raise SystemExit(2)
    major, minor, patch = map(int, match.group(1, 2, 3))
    prerelease = match.group(4)
    return (major, minor, patch, prerelease is None, prerelease or "")

raise SystemExit(0 if key(sys.argv[2]) > key(sys.argv[1]) else 1)
PY
then
  echo "agent-check: refusing non-forward Pi promotion $CURRENT -> $LATEST" >&2
  exit 1
fi

SUMS_URL="$(python3 - "$release_json" <<'PY'
import json, sys
release = json.load(open(sys.argv[1], encoding="utf-8"))
for asset in release.get("assets", []):
    if asset.get("name") == "SHA256SUMS":
        print(asset.get("browser_download_url", ""))
        break
PY
)"
[[ -n "$SUMS_URL" ]] || { echo "agent-check: release has no SHA256SUMS asset" >&2; exit 1; }
curl --fail --silent --show-error --location --retry 3 "$SUMS_URL" -o "$sums"

sha_for() {
  local asset="$1"
  awk -v asset="$asset" '$2 == asset {print $1}' "$sums"
}
SHA_X64="$(sha_for pi-linux-x64.tar.gz)"
SHA_ARM64="$(sha_for pi-linux-arm64.tar.gz)"
[[ "$SHA_X64" =~ ^[0-9a-f]{64}$ && "$SHA_ARM64" =~ ^[0-9a-f]{64}$ ]] || {
  echo "agent-check: release checksum manifest is incomplete" >&2
  exit 1
}

case "$(uname -m)" in
  x86_64) LOCAL_SHA="$SHA_X64" ;;
  aarch64|arm64) LOCAL_SHA="$SHA_ARM64" ;;
  *) echo "agent-check: unsupported local architecture: $(uname -m)" >&2; exit 2 ;;
esac

HOME="$check_home" "$SCRIPT_DIR/pi-runtime.sh" --version "$LATEST" --sha256 "$LOCAL_SHA"
echo "agent-check: candidate $LATEST passed deterministic runtime compatibility gate"

if [[ "$UPDATE_LOCK" == "true" ]]; then
  python3 - "$MANIFEST" "$LATEST" "$SHA_X64" "$SHA_ARM64" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
version, sha_x64, sha_arm64 = sys.argv[2:]
text = path.read_text(encoding="utf-8")
replacements = {
    "pi": version,
    "pi-linux-x64-sha256": sha_x64,
    "pi-linux-arm64-sha256": sha_arm64,
}
for key, value in replacements.items():
    pattern = rf'(?m)^({re.escape(key)}\s*=\s*")[^"]*(".*)$'
    text, count = re.subn(pattern, rf'\g<1>{value}\g<2>', text, count=1)
    if count != 1:
        raise SystemExit(f"failed to update unique {key} pin")
path.write_text(text, encoding="utf-8")
PY
  echo "agent-check: updated $MANIFEST to Pi $LATEST"
fi
