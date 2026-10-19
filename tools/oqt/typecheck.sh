#!/usr/bin/env bash
# Typecheck the owned composition inside a prepared private derivative (the
# patched upstream files included). Usage: tools/oqt/typecheck.sh [PI_SOURCE]
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source=${1:-${OQT_PI_SOURCE:-$HOME/.local/share/oqt/pi-source}}
work=$(mktemp -d /tmp/oqt-typecheck.XXXX)
trap 'rm -rf "$work"' EXIT
mkdir "$work/owner"
prepared=$(python3 "$root/tools/oqt/prepare.py" "$source" "$work" coding "$work/owner")
python3 - "$prepared" "$source" "$work/tsconfig.json" <<'PY'
import json, sys
from pathlib import Path
prepared, source, out = json.loads(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
snap = Path(prepared['source'])
paths = {k: [str(source / p) for p in v] for k, v in json.loads((source / 'tsconfig.base.json').read_text())['compilerOptions'].get('paths', {}).items()}
for pkg, folder in [('pi-ai', 'ai'), ('pi-durable', 'durable'), ('pi-env', 'env'), ('pi-durable/env', 'durable')]:
    pass
paths.update({
    '@earendil-works/pi-ai': [str(source / 'packages/ai/src/index.ts')],
    '@earendil-works/pi-ai/*': [str(source / 'packages/ai/src/*.ts'), str(source / 'packages/ai/src/*/index.ts')],
    '@earendil-works/pi-durable': [str(source / 'packages/durable/src/index.ts')],
    '@earendil-works/pi-durable/env': [str(source / 'packages/durable/src/env/index.ts')],
    '@earendil-works/pi-durable/tools': [str(source / 'packages/durable/src/tools/index.ts')],
    '@earendil-works/pi-durable/storage/sqlite/node': [str(source / 'packages/durable/src/storage/sqlite/node.ts')],
    '@earendil-works/pi-env': [str(source / 'packages/env/src/index.ts')],
    '@earendil-works/chord': [str(source / 'node_modules/@earendil-works/chord/dist/index.d.ts')],
    '@earendil-works/chord/*': [str(source / 'node_modules/@earendil-works/chord/dist/*/index.d.ts')],
    '@earendil-works/pi-coding-agent/experimental/plugin': [str(snap / 'packages/coding-agent/src/experimental/plugin.ts')],
})
experimental = snap / 'packages/coding-agent/src/experimental'
files = [str(experimental / m) for m in ['oqt-runtime-worker.ts', 'oqt-launcher.ts', 'coding-worker.ts', 'host.ts', 'guard.ts',
         'spawn-hook.ts', 'session-commands.ts', 'owner-refresh.ts', 'owner-refresh-core.ts', 'classify.ts', 'session-worker-manager.ts',
         'client-tui.ts', 'server.ts', 'session-worker.ts']]
files += [str(snap / 'packages/coding-agent/examples/plugins/oq-permissions/src/tui.ts'),
          str(snap / 'packages/coding-agent/examples/plugins/oq-permissions/src/models.ts'),
          str(snap / 'packages/coding-agent/src/utils/highlight-js.d.ts')]
out.write_text(json.dumps({'extends': str(source / 'tsconfig.base.json'), 'compilerOptions': {
    'module': 'ESNext', 'moduleResolution': 'Bundler', 'noEmit': True, 'declaration': False, 'declarationMap': False,
    'verbatimModuleSyntax': False, 'allowImportingTsExtensions': True,
    'typeRoots': [str(source / 'node_modules/@types')], 'paths': paths}, 'files': files}))
PY
cpu=$(python3 -c 'import os; print(min(os.sched_getaffinity(0)))')
timeout 200 taskset -c "$cpu" nice -n 19 "$source/node_modules/.bin/tsc" -p "$work/tsconfig.json"
echo "typecheck: clean"
