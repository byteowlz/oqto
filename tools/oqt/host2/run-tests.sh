#!/usr/bin/env bash
# Run host2 tests against current upstream Pi (never the pinned runtime).
# Usage: tools/oqt/host2/run-tests.sh [UPSTREAM_WORKTREE]
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
upstream=${1:-${OQT_PI_UPSTREAM:-$HOME/.local/share/oqt/pi-upstream}}
work=$(mktemp -d /var/tmp/oqt-host2-run.XXXXXX)
trap 'rm -rf -- "$work"' EXIT
git -C "$upstream" archive HEAD | tar -x -C "$work"
ln -s "$upstream/node_modules" "$work/node_modules"
# npm workspaces keep their own nested node_modules (versions differ from the root).
for nested in "$upstream"/packages/*/node_modules; do
  [ -d "$nested" ] && ln -s "$nested" "$work/${nested#"$upstream"/}"
done
[ -d "$upstream/packages/ai/src/providers/data" ] && cp -r "$upstream/packages/ai/src/providers/data" "$work/packages/ai/src/providers/" || true
target="$work/packages/coding-agent/src/oqt-host2"
mkdir -p "$target"
cp "$root"/tools/oqt/host2/*.ts "$target/"
# Portable oqt modules (no retired stack imports), placed where they import from.
portable=(guard coding-runtime brain-client hands-client hands-supervisor gated-tool coding-authority
  carrier classify guard-errors guard-wire input-leases operator-receipts owner-broker trace
  permission-feedback execution-bindings environment-tools coding-composition owner-models classic-import session-forks classic-import-host classic-import-worker classic-source readonly-profile program-status presentation-startup)
for module in "${portable[@]}"; do cp "$root/tools/oqt/$module.ts" "$work/packages/coding-agent/src/experimental/"; done
# Brain-side hands transport proxy (spawned by HandsClient beside hands-client.ts).
cp "$root/tools/oqt/hands-proxy.mjs" "$work/packages/coding-agent/src/experimental/"
resolver="$work/packages/coding-agent/src/experimental/source-resolver.ts"
cd "$work"
cat > "$work/oqt-host2.tsconfig.json" <<JSON
{ "extends": "./tsconfig.json", "compilerOptions": { "noEmit": true, "allowImportingTsExtensions": true },
  "include": ["packages/coding-agent/src/oqt-host2/*.ts", "packages/coding-agent/src/experimental/*.ts", "packages/coding-agent/src/**/*.d.ts"] }
JSON
timeout 300s node_modules/.bin/tsc -p "$work/oqt-host2.tsconfig.json"
echo "host2 typecheck: clean"
# Confined-brain tests use the built confinement assets when present (else skip).
target_dir="$root/backend/target"
for pair in OQT_BRAIN=debug/examples/oqt_brain OQT_HANDS=debug/examples/oqt_hands \
  OQT_SANDBOX_SHIM=debug/examples/oqt_shim OQT_CARRIER=debug/examples/permission_carrier \
  OQT_ENV_DAEMON=oqt-env/debug/pi-env OQT_SECCOMP_BPF=oqt-prepared/bpf/default-x86_64.bpf; do
  name=${pair%%=*}; path="$target_dir/${pair#*=}"
  [ -n "${!name:-}" ] || { [ -e "$path" ] && export "$name=$path"; }
done
export OQTO_SANDBOX_SHIM_BIN="${OQT_SANDBOX_SHIM:-}"
OQT_HOST2_SNAPSHOT="$work" OQT_PI_UPSTREAM_ROOT="$upstream" OQT_SOURCE_RESOLVER="$resolver" timeout --kill-after=5s 240s \
  node --import "$resolver" --test --test-concurrency=1 "$target"/*.test.ts
