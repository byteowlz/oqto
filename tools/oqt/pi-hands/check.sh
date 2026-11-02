#!/usr/bin/env bash
# Explicit-load extension proof against a caller-selected Pi SDK source tree.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
source=${OQT_PI_SOURCE:-$HOME/.local/share/oqt/pi-source}
work=$(mktemp -d /var/tmp/pi-hands-check.XXXXXX)
trap 'rm -rf -- "$work"' EXIT
python3 - "$root" "$source" "$work/tsconfig.json" <<'PY'
import json, sys
from pathlib import Path
root, source, output = map(Path, sys.argv[1:])
base=json.loads((source/'tsconfig.base.json').read_text())
paths={k:[str(source/p) for p in values] for k,values in base['compilerOptions'].get('paths',{}).items()}
paths.update({'@earendil-works/pi-ai':[str(source/'packages/ai/dist/index.d.ts')],
              '@earendil-works/pi-ai/*':[str(source/'packages/ai/dist/*.d.ts')],
              '@earendil-works/pi-coding-agent':[str(source/'packages/coding-agent/dist/index.d.ts')]})
output.write_text(json.dumps({'extends':str(source/'tsconfig.base.json'),
 'compilerOptions':{'module':'ESNext','moduleResolution':'Bundler','verbatimModuleSyntax':False,'noEmit':True,'allowImportingTsExtensions':True,'paths':paths,
 'typeRoots':[str(source/'node_modules/@types')]},
 'files':[str(root/'tools/oqt/pi-hands/index.ts')]}))
PY
timeout 200 taskset -c 0 nice -n 19 "$source/node_modules/.bin/tsc" -p "$work/tsconfig.json"
