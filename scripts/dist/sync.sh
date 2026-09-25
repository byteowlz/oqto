#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

DIST_CACHE_DIR="${OQTO_DIST_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/oqto/dist-sources}"
OQTO_TEMPLATES_URL="${OQTO_TEMPLATES_URL:-https://github.com/byteowlz/oqto-templates.git}"
PI_AGENT_EXTENSIONS_URL="${PI_AGENT_EXTENSIONS_URL:-https://github.com/byteowlz/pi-agent-extensions.git}"
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
  local name="$1" local_path="$2" url="$3" ref="$4"
  local cached_path="$DIST_CACHE_DIR/$name"

  if [[ -e "$local_path" ]]; then
    local actual expected
    actual="$(git -C "$local_path" rev-parse HEAD 2>/dev/null)" || {
      echo "error: $name at $local_path is not a verifiable git checkout" >&2
      return 1
    }
    expected="$(git -C "$local_path" rev-parse "${ref}^{commit}" 2>/dev/null)" || {
      echo "error: pinned $name commit $ref is unavailable in $local_path" >&2
      return 1
    }
    [[ "$actual" == "$expected" && -z "$(git -C "$local_path" status --porcelain)" ]] || {
      echo "error: $name at $local_path is dirty or does not match pinned commit $ref" >&2
      return 1
    }
    echo "$local_path"
    return 0
  fi

  if ! command -v git >/dev/null 2>&1; then
    echo "error: $name source missing at $local_path and git is not available to fetch $url" >&2
    return 1
  fi

  mkdir -p "$DIST_CACHE_DIR"
  if [[ ! -d "$cached_path/.git" ]]; then
    [[ ! -e "$cached_path" ]] || {
      echo "error: refusing to overwrite unknown $name cache at $cached_path" >&2
      return 1
    }
    echo "[dist-sync] $name missing at $local_path; cloning $url into $cached_path" >&2
    git clone --filter=blob:none "$url" "$cached_path" >&2
  else
    echo "[dist-sync] $name missing at $local_path; updating cached checkout $cached_path" >&2
    git -C "$cached_path" fetch --tags --prune origin >&2
  fi

  [[ -z "$(git -C "$cached_path" status --porcelain)" ]] || {
    echo "error: refusing to switch dirty $name cache at $cached_path" >&2
    return 1
  }
  git -C "$cached_path" checkout --detach "$ref" >&2
  local actual expected
  actual="$(git -C "$cached_path" rev-parse HEAD)"
  expected="$(git -C "$cached_path" rev-parse "${ref}^{commit}")"
  [[ "$actual" == "$expected" && -z "$(git -C "$cached_path" status --porcelain)" ]] || {
    echo "error: cached $name is dirty or not at pinned commit $ref" >&2
    return 1
  }
  echo "$cached_path"
}

TEMPLATES_REPO="$(resolve_source_repo \
  oqto-templates \
  "${OQTO_TEMPLATES_REPO:-$ROOT_DIR/../oqto-templates}" \
  "$OQTO_TEMPLATES_URL" \
  "$OQTO_TEMPLATES_REF")"
EXTENSIONS_REPO="$(resolve_source_repo \
  pi-agent-extensions \
  "${PI_AGENT_EXTENSIONS_REPO:-$ROOT_DIR/../pi-agent-extensions}" \
  "$PI_AGENT_EXTENSIONS_URL" \
  "$PI_AGENT_EXTENSIONS_REF")"

mkdir -p dist/immutable/defaults/pi-agent \
         dist/immutable/defaults/pi-agent/extensions \
         dist/immutable/defaults/workdir-templates \
         dist/immutable/defaults/onboarding-templates/onboarding

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
EXTENSIONS=(
  pi-auto-rename
  pi-custom-context-files
  pi-env-ctx
  pi-error-recovery
  pi-history-search
  pi-introspection
  pi-markdown-export
  pi-oqto-bridge
  pi-oqto-todos
)

for ext in "${EXTENSIONS[@]}"; do
  src="$EXTENSIONS_REPO/$ext"
  dst="dist/immutable/defaults/pi-agent/extensions/$ext"
  if [[ ! -d "$src" ]]; then
    echo "error: missing extension in source repo: $src" >&2
    exit 1
  fi
  rm -rf "$dst"
  mkdir -p "$dst"
  cp -R "$src/." "$dst/"
done

echo "dist sync complete"
