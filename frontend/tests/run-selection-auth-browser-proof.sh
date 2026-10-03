#!/usr/bin/env bash
# Opt-in only: requires a ready disposable REAL gateway contract, synthetic JWTs.
set -euo pipefail
umask 077
contract="${1:?Usage: run-selection-auth-browser-proof.sh private-contract.json [proof-directory]}"
proof="${2:-$(mktemp -d /tmp/oqto-selection-auth-browser.XXXXXX)}"
contract="$(realpath -- "$contract")"
proof="$(realpath -m -- "$proof")"
frontend="$(cd -- "$(dirname -- "$0")/.." && pwd)"
mkdir -p "$proof/build"
cd "$frontend"
bun build tests/selection-auth-browser-fixture.tsx --bundle --format iife --target browser --outdir "$proof/build"
cp tests/selection-auth-browser.html "$proof/build/index.html"
python - "$contract" "$proof" <<'PY'
import json, subprocess, sys
from pathlib import Path
contract, proof = map(Path, sys.argv[1:])
c = json.loads(contract.read_text())
if not (contract.parent / 'ready').exists():
    raise SystemExit('Gateway has not passed readiness: missing private ready file')
args = ['python', 'tests/selection-auth-browser-proof.py',
        '--build-dir', str(proof / 'build'), '--gateway', c['http_origin'],
        '--session', c['session_id'], '--token', c['tokens'][0],
        '--foreign-session', c['other_session_id'], '--foreign-token', c['tokens'][1],
        '--output', str(proof)]
raise SystemExit(subprocess.call(args))
PY
