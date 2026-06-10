#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

TEMPLATES_REPO="${OQTO_TEMPLATES_REPO:-$ROOT_DIR/../oqto-templates}"
EXTENSIONS_REPO="${PI_AGENT_EXTENSIONS_REPO:-$ROOT_DIR/../pi-agent-extensions}"

if [[ ! -d "$TEMPLATES_REPO" ]]; then
  echo "error: oqto-templates repo not found at $TEMPLATES_REPO" >&2
  exit 1
fi
if [[ ! -d "$EXTENSIONS_REPO" ]]; then
  echo "error: pi-agent-extensions repo not found at $EXTENSIONS_REPO" >&2
  exit 1
fi

mkdir -p dist/immutable/defaults/pi-agent \
         dist/immutable/defaults/pi-agent/extensions \
         dist/immutable/defaults/workdir-templates \
         dist/immutable/defaults/onboarding-templates/onboarding

# Sync canonical template families from oqto-templates.
cp "$TEMPLATES_REPO/pi-agent/AGENTS.md" dist/immutable/defaults/pi-agent/AGENTS.md
if [[ -f "$TEMPLATES_REPO/pi-agent/settings.json" ]]; then
  cp "$TEMPLATES_REPO/pi-agent/settings.json" dist/immutable/defaults/pi-agent/settings.json
elif [[ -f .pi/settings.json ]]; then
  cp .pi/settings.json dist/immutable/defaults/pi-agent/settings.json
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
