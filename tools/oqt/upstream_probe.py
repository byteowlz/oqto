#!/usr/bin/env python3
"""Assemble current upstream Pi + oqt host-stack + oqt modules and typecheck it.

Usage: upstream_probe.py [UPSTREAM_SOURCE] [--ref origin/main] [--out DIR]
Never touches the pinned runtime source. Reports patches that no longer apply
and TypeScript errors grouped by file, so porting work is measured, not guessed.
Exit 0 only when every patch applies and the typecheck is clean.
"""
import argparse
import collections
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
OQT = ROOT/'tools/oqt'
sys.path.insert(0, str(OQT))
STACK_PATHS = {
    '@earendil-works/pi-client': ['packages/client/src/index.ts'],
    '@earendil-works/pi-client/*': ['packages/client/src/*.ts', 'packages/client/src/*/index.ts'],
    '@earendil-works/pi-server': ['packages/server/src/index.ts'],
    '@earendil-works/pi-server/*': ['packages/server/src/*.ts', 'packages/server/src/*/index.ts'],
    '@earendil-works/pi-protocol': ['packages/protocol/src/index.ts'],
    '@earendil-works/pi-protocol/*': ['packages/protocol/src/*.ts', 'packages/protocol/src/*/index.ts'],
}


def assemble(upstream, ref, out):
    if out.exists():
        for directory, _, _ in os.walk(out):
            os.chmod(directory, 0o700)
        shutil.rmtree(out)
    snap = out/'pi-source'
    snap.mkdir(parents=True)
    archive = subprocess.run(['git', '-C', str(upstream), 'archive', ref], check=True, capture_output=True).stdout
    subprocess.run(['tar', '-x', '-C', str(snap)], input=archive, check=True)
    (snap/'node_modules').symlink_to(upstream/'node_modules', target_is_directory=True)
    data = upstream/'packages/ai/src/providers/data'
    if data.exists():
        shutil.copytree(data, snap/'packages/ai/src/providers/data', dirs_exist_ok=True)
    stack = OQT/'host-stack/packages'  # optional overlay; absent once ported to chord2
    if stack.exists():
        shutil.copytree(stack, snap/'packages', dirs_exist_ok=True)
    config = snap/'tsconfig.json'
    tsconfig = json.loads(config.read_text())
    tsconfig['compilerOptions']['paths'].update(STACK_PATHS)
    config.write_text(json.dumps(tsconfig, indent=2))
    return snap


def apply_patches(snap):
    import source_patches
    failed = []
    for patch in source_patches.PATCHES:
        path = snap/patch['file']
        text = path.read_text() if path.exists() else ''
        if text.count(patch['old']) != 1:
            failed.append((patch['id'], patch['file']))
            continue
        path.write_text(text.replace(patch['old'], patch['new']))
    for module in ('tool_presentation_patches', 'early_presentation_patches'):
        try:
            __import__(module).apply(snap)
        except SystemExit as error:
            failed.append((module, str(error)))
    experimental = snap/'packages/coding-agent/src/experimental'
    worker = experimental/'session-worker.ts'
    original = 'return runSessionWorkerWithHarness(args, createCodingAgentHarness);'
    text = worker.read_text()
    if text.count(original) == 1:
        worker.write_text(text.replace(original, 'return import("./oqt-runtime-worker.ts").then(({createOqtHarness}) => runSessionWorkerWithHarness(args, createOqtHarness));'))
    else:
        failed.append(('worker-entry', 'session-worker.ts'))
    for module in OQT.glob('*.ts'):
        if module.name.endswith('.test.ts'):
            continue
        name = {'worker.ts': 'oqt-runtime-worker.ts', 'launcher.ts': 'oqt-launcher.ts'}.get(module.name, module.name)
        shutil.copyfile(module, experimental/name)
    shutil.copytree(OQT/'plugin', snap/'packages/coding-agent/examples/plugins/oq-permissions', dirs_exist_ok=True)
    return failed


def typecheck(upstream, snap, out):
    experimental = snap/'packages/coding-agent/src/experimental'
    files = [str(experimental/m) for m in ['oqt-runtime-worker.ts', 'oqt-launcher.ts', 'coding-worker.ts', 'host.ts', 'guard.ts',
             'spawn-hook.ts', 'session-commands.ts', 'owner-refresh.ts', 'owner-refresh-core.ts', 'classify.ts',
             'session-worker-manager.ts', 'client-tui.ts', 'server.ts', 'session-worker.ts', 'presentation-client.ts']]
    files += [str(snap/'packages/coding-agent/examples/plugins/oq-permissions/src'/m) for m in ('tui.ts', 'models.ts')]
    highlight = snap/'packages/coding-agent/src/utils/highlight-js.d.ts'
    if highlight.exists():
        files.append(str(highlight))
    paths = {key: [str(snap/value) for value in values]
             for key, values in json.loads((snap/'tsconfig.json').read_text())['compilerOptions']['paths'].items()}
    paths['@earendil-works/pi-coding-agent/experimental/plugin'] = [str(experimental/'plugin.ts')]
    config = out/'tsconfig.json'
    config.write_text(json.dumps({'extends': str(snap/'tsconfig.base.json'), 'compilerOptions': {
        'module': 'ESNext', 'moduleResolution': 'Bundler', 'noEmit': True, 'declaration': False, 'declarationMap': False,
        'verbatimModuleSyntax': False, 'allowImportingTsExtensions': True,
        'typeRoots': [str(upstream/'node_modules/@types')], 'paths': paths}, 'files': files}))
    result = subprocess.run([str(upstream/'node_modules/.bin/tsc'), '-p', str(config)], capture_output=True, text=True, timeout=600)
    return result.stdout + result.stderr


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('upstream', nargs='?', default=str(Path.home()/'.local/share/oqt/pi-upstream'))
    parser.add_argument('--ref', default='origin/main')
    parser.add_argument('--out', default='/var/tmp/oqt-upstream-probe')
    options = parser.parse_args()
    upstream, out = Path(options.upstream).resolve(), Path(options.out)
    snap = assemble(upstream, options.ref, out)
    failed = apply_patches(snap)
    output = typecheck(upstream, snap, out)
    (out/'tsc.log').write_text(output)
    errors = [line for line in output.splitlines() if re.search(r'error TS\d+', line)]
    by_file = collections.Counter(re.sub(r'\(\d+,\d+\).*', '', line).split('/pi-source/')[-1] for line in errors)
    codes = collections.Counter(re.search(r'error (TS\d+)', line).group(1) for line in errors)
    print(f'ref {options.ref}: {len(failed)} patches no longer apply, {len(errors)} type errors in {len(by_file)} files')
    for patch, where in failed:
        print(f'  patch {patch}: {where}')
    for name, count in by_file.most_common(25):
        print(f'  {count:4d} {name}')
    print('  codes:', ', '.join(f'{code}x{count}' for code, count in codes.most_common(8)))
    print(f'  full log: {out}/tsc.log')
    return 0 if not failed and not errors else 1


if __name__ == '__main__':
    sys.exit(main())
