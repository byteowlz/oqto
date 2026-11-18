#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
python3 scripts/dist/verify-worktree-ownership.py --root "$ROOT_DIR"

DIST_CACHE_DIR="${OQTO_DIST_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/oqto/dist-sources}"
OQTO_TEMPLATES_URL="${OQTO_TEMPLATES_URL:-https://github.com/byteowlz/oqto-templates.git}"
# shellcheck source=scripts/dist/pi-ext-sources.sh
source "$ROOT_DIR/scripts/dist/pi-ext-sources.sh"
# Public source is the default. Development builds can opt into a private source
# with PI_AGENT_EXTENSIONS_URL; release pins must still exist on the public mirror.
# Public history was rewritten, so some pins only exist there (pi-ext-sources.sh).
PI_AGENT_EXTENSIONS_URL="${PI_AGENT_EXTENSIONS_URL:-$PI_EXT_PUBLIC_URL_DEFAULT}"
PI_AGENT_EXTENSIONS_PUBLIC_URL="${PI_AGENT_EXTENSIONS_PUBLIC_URL:-$PI_EXT_PUBLIC_URL_DEFAULT}"
PI_AGENT_EXTENSIONS_URLS="$PI_AGENT_EXTENSIONS_URL $PI_AGENT_EXTENSIONS_PUBLIC_URL"
# The release and the installed Pi runtime must use the same locked inputs.
# Never package a moving `main` checkout as though it were a reproducible build.
read_source_pin() {
  local key="$1"
  awk -F '"' -v key="$key" '
    /^\[external\]/ { in_external = 1; next }
    /^\[/ { in_external = 0 }
    in_external && $1 ~ "^[[:space:]]*" key "[[:space:]]*=" { print $2; exit }
  ' "$ROOT_DIR/dependencies.toml"
}

pinned_templates="$(read_source_pin oqto-templates)"
pinned_extensions="$(read_source_pin pi-extensions)"
[[ "$pinned_templates" =~ ^[0-9a-f]{7,40}$ && "$pinned_extensions" =~ ^[0-9a-f]{7,40}$ ]] || {
  echo 'error: dependencies.toml must pin oqto-templates and pi-extensions to commit IDs' >&2
  exit 1
}
OQTO_TEMPLATES_REF="${OQTO_TEMPLATES_REF:-$pinned_templates}"
PI_AGENT_EXTENSIONS_REF="${PI_AGENT_EXTENSIONS_REF:-$pinned_extensions}"
[[ "$OQTO_TEMPLATES_REF" == "$pinned_templates" && "$PI_AGENT_EXTENSIONS_REF" == "$pinned_extensions" ]] || {
  echo 'error: source ref overrides must match dependencies.toml; update the pin first' >&2
  exit 1
}
if [[ "${1:-}" == '--print-refs' ]]; then
  printf 'oqto-templates=%s\npi-agent-extensions=%s\n' "$OQTO_TEMPLATES_REF" "$PI_AGENT_EXTENSIONS_REF"
  exit 0
fi
[[ $# -eq 0 ]] || { echo "error: unknown sync arg: $1" >&2; exit 2; }

resolve_source_repo() {
  local name="$1" local_path="$2" urls="$3" ref="$4"
  pi_ext_resolve_source "$DIST_CACHE_DIR/$name" "$local_path" "$urls" "$ref" "$name"
}

TEMPLATES_REPO="$(resolve_source_repo \
  oqto-templates \
  "${OQTO_TEMPLATES_REPO:-$ROOT_DIR/../oqto-templates}" \
  "$OQTO_TEMPLATES_URL" \
  "$OQTO_TEMPLATES_REF")"
EXTENSIONS_REPO="$(resolve_source_repo \
  pi-agent-extensions \
  "${PI_AGENT_EXTENSIONS_REPO:-$ROOT_DIR/../pi-agent-extensions}" \
  "$PI_AGENT_EXTENSIONS_URLS" \
  "$PI_AGENT_EXTENSIONS_REF")"

mkdir -p dist/immutable/defaults/pi-agent \
         dist/immutable/defaults/pi-agent/extensions \
         dist/immutable/defaults/workdir-templates \
         dist/immutable/defaults/onboarding-templates/onboarding

# Runner-owned auth helper; no credentials or user configuration are shipped.
mkdir -p dist/immutable/provider-login
for file in worker.mjs broker.mjs pi-runtime.mjs README.md; do
  cp "tools/provider-login/$file" "dist/immutable/provider-login/$file"
done

# Sync canonical template families from oqto-templates.
cp "$TEMPLATES_REPO/pi-agent/AGENTS.md" dist/immutable/defaults/pi-agent/AGENTS.md
if [[ -f "$TEMPLATES_REPO/pi-agent/settings.json" ]]; then
  cp "$TEMPLATES_REPO/pi-agent/settings.json" dist/immutable/defaults/pi-agent/settings.json
else
  # Never copy the developer checkout's .pi/settings.json into release
  # defaults. Model/provider defaults are user/admin/EAVS policy, not a
  # globally shipped template opinion.
  printf '{}\n' > dist/immutable/defaults/pi-agent/settings.json
fi

# Workdir templates: sync full tree.
rm -rf dist/immutable/defaults/workdir-templates
mkdir -p dist/immutable/defaults/workdir-templates
cp -R "$TEMPLATES_REPO/workdir-templates/." dist/immutable/defaults/workdir-templates/

# Keep a stable "default" alias expected by current manifest.
if [[ ! -d dist/immutable/defaults/workdir-templates/default ]]; then
  if [[ -d dist/immutable/defaults/workdir-templates/main ]]; then
    cp -R dist/immutable/defaults/workdir-templates/main \
      dist/immutable/defaults/workdir-templates/default
  else
    first_template="$(find dist/immutable/defaults/workdir-templates -mindepth 1 -maxdepth 1 -type d | head -n1 || true)"
    if [[ -n "$first_template" ]]; then
      cp -R "$first_template" dist/immutable/defaults/workdir-templates/default
    fi
  fi
fi

# Onboarding templates.
cp -R "$TEMPLATES_REPO/onboarding-templates/onboarding/." dist/immutable/defaults/onboarding-templates/onboarding/

# Canonical shipped extension set (must stay in sync with dist/manifest.toml [pi_agent_extensions].items)
mapfile -t EXTENSIONS < <(python3 - <<'PY'
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
with open("dist/manifest.toml", "rb") as source:
    for item in tomllib.load(source)["pi_agent_extensions"]["items"]:
        print(item)
PY
)
[[ ${#EXTENSIONS[@]} -gt 0 ]] || { echo 'error: no canonical extensions in manifest' >&2; exit 1; }

# Validate the complete source set before replacing build-owned output. This is
# NOT a user extension directory: retired entries here must not survive a new
# release and collide with their replacement tools during native autoload.
for ext in "${EXTENSIONS[@]}"; do
  [[ "$ext" =~ ^[a-zA-Z0-9][a-zA-Z0-9_-]*$ && -d "$EXTENSIONS_REPO/$ext" ]] || {
    echo "error: invalid or missing extension in source repo: $ext" >&2
    exit 1
  }
done
rm -rf "$ROOT_DIR/dist/immutable/defaults/pi-agent/extensions"
mkdir -p "$ROOT_DIR/dist/immutable/defaults/pi-agent/extensions"
for ext in "${EXTENSIONS[@]}"; do
  src="$EXTENSIONS_REPO/$ext"
  dst="dist/immutable/defaults/pi-agent/extensions/$ext"
  mkdir -p "$dst"
  cp -R "$src/." "$dst/"
done

echo "dist sync complete"
