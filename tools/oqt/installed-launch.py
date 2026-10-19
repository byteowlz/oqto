#!/usr/bin/python3
"""Launch one verified, resolved experimental runtime; never execute checkout code."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform
import sys
import time


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def overlaps(left, right):
    return left == right or left.is_relative_to(right) or right.is_relative_to(left)


def main():
    started = str(time.time())
    runtime = Path(__file__).resolve().parent
    sys.path.insert(0, str(runtime))
    verifier = load('oqt_runtime_verifier', runtime/'install-runtime.py')
    verifier.verify_runtime(runtime)
    metadata = json.loads((runtime/'runtime.json').read_text())
    def asset(name):
        value = Path(metadata[name])
        value = (runtime/value).resolve(strict=True) if not value.is_absolute() else value.resolve(strict=True)
        if not value.is_relative_to(runtime):
            raise RuntimeError(f'Runtime path escapes installation: {name}')
        return str(value)
    source = Path(asset('source'))
    node = asset('node')
    arguments = sys.argv[1:]
    if arguments and arguments[0] in ('inspect-classic', 'list-classic'):
        os.execv(node, [node, str(source/'packages/coding-agent/src/experimental/classic-inspect.ts'), *arguments])
    parser = argparse.ArgumentParser(description='Installed experimental oqt: confined Pi Durable workspace coding.')
    parser.add_argument('--workdir', default=os.getcwd())
    parser.add_argument('--owner-profile', default=os.environ.get('OQT_OWNER_AGENT_DIR') or os.environ.get('PI_CODING_AGENT_DIR') or str(Path.home()/'.pi/agent'))
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--coding', action='store_true')
    mode.add_argument('--demo', action='store_true')
    parser.add_argument('--continue', '-c', dest='continuing', action='store_true')
    parser.add_argument('--smoke', action='store_true')
    parser.add_argument('--json', action='store_true')
    parser.add_argument('--import-classic')
    parser.add_argument('--origin')
    parser.add_argument('--leaf')
    parser.add_argument('--runtime-info', action='store_true', help='Print pinned runtime identity without opening a Session')
    options = parser.parse_args(arguments)
    if options.runtime_info:
        print(json.dumps({'runtime': str(runtime), 'artifact': runtime.name, 'source': str(source), 'node': node}))
        return
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        raise RuntimeError('This experimental runtime requires Linux x86_64')
    workdir = Path(options.workdir).expanduser().resolve(strict=True)
    if not workdir.is_dir():
        raise RuntimeError('Work directory must be a directory')
    install_store = runtime.parent.parent
    state_home = Path(os.environ.get('XDG_STATE_HOME', str(Path.home()/'.local/state'))).expanduser()
    if not state_home.is_absolute():
        raise RuntimeError('XDG_STATE_HOME must be absolute')
    state_store = (state_home/'oqt').resolve()
    state_module = load('oqt_installed_state', runtime/'installed-state.py')
    ipc_alias = state_module.ipc_alias(state_home, runtime.name)
    if any(overlaps(workdir, protected) for protected in (install_store, state_store, ipc_alias)):
        raise RuntimeError('Workspace overlaps protected installed runtime or state; choose a narrower workspace')
    owner = Path(options.owner_profile).expanduser().resolve(strict=not options.demo)
    mode = 'demo' if options.demo else 'coding'
    state = state_module.prepare_state(runtime, runtime.name, workdir, mode, owner, state_home)
    args = ['--demo'] if options.demo else []
    for flag, enabled in [('--continue', options.continuing), ('--smoke', options.smoke), ('--json', options.json)]:
        if enabled:
            args.append(flag)
    if options.import_classic:
        args.extend(['--import-classic', str(Path(options.import_classic).expanduser().resolve())])
    for flag, value in [('--origin', options.origin), ('--leaf', options.leaf)]:
        if value is not None:
            args.extend([flag, value])
    env = {
        'PATH': os.environ.get('PATH', '/usr/bin:/bin'), 'HOME': state['home'],
        'LANG': os.environ.get('LANG', 'C.UTF-8'), 'TERM': os.environ.get('TERM', 'xterm-256color'),
        'COLORTERM': os.environ.get('COLORTERM', ''), 'NODE_OPTIONS': '--max-old-space-size=512',
        'PI_EXPERIMENTAL': '1', 'PI_OFFLINE': '1', 'PI_CODING_AGENT_DIR': state['agent'],
        'OQT_MODE': mode, 'OQT_OWNER_AGENT_DIR': str(owner), 'OQT_RUNTIME_ROOT': state['runtime_root'],
        'OQT_TRUSTED_SOURCE_ROOT': str(install_store), 'OQT_SOURCE_CHECKOUT': str(source),
        'OQT_INSTALLED_BIN_ROOT': str(Path.home()/'.local/bin'),
        'OQT_SERVER_DIR': state['profile'], 'OQT_CARRIER': asset('carrier'), 'OQT_HANDS': asset('hands'),
        'OQT_BRAIN': asset('brain'), 'OQT_ENV_DAEMON': asset('daemon'), 'OQT_SANDBOX_SHIM': asset('shim'),
        'OQT_SECCOMP_BPF': asset('bpf'),
        'OQT_SANDBOX_CONFIG': os.environ.get('OQT_SANDBOX_CONFIG', str(Path.home()/'.config/oqto/sandbox.toml')),
        'OQT_INPUT_TRACE': state['profile']+'/input-trace.log', 'OQT_STARTUP_TRACE': state['profile']+'/startup-trace.log',
        'OQT_SHELL_START': started, 'NODE_COMPILE_CACHE': state['runtime_root']+'/node-compile-cache',
    }
    for key in ('OQT_DEBUG', 'OQT_HOST_IDLE_SECONDS'):
        if key in os.environ:
            env[key] = os.environ[key]
    os.chdir(workdir)
    os.execve(node, [node, '--import', str(source/'packages/coding-agent/src/experimental/source-resolver.ts'),
                    str(source/'packages/coding-agent/src/experimental/oqt-launcher.ts'), *args], env)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, KeyError) as error:
        print(f'oqt: {error}', file=sys.stderr)
        sys.exit(2)
