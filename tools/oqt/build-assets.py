#!/usr/bin/env python3
"""Warm preparation cache only; artifacts are not execution authority.

Platforms: Linux x86_64 (bwrap, seccomp BPF, Landlock) and macOS arm64
(Seatbelt). There is no seccomp on macOS, so no filter is compiled there and
the last output line is empty instead of a BPF path."""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
from preparation_lock import acquire_preparation_lock

PLATFORMS = {('Linux', 'x86_64'): 'linux-x64', ('Darwin', 'arm64'): 'darwin-arm64'}
target = PLATFORMS.get((platform.system(), platform.machine()))
if target is None:
    raise SystemExit(f'oqt supports Linux x86_64 and macOS arm64, not {platform.system()} {platform.machine()}')
seccomp = target == 'linux-x64'
ROOT=Path(__file__).resolve().parents[2]
# OQT_TARGET_DIR keeps a developer's build out of a shared checkout's target.
TARGET=Path(os.environ.get('OQT_TARGET_DIR') or ROOT/'backend/target')
CACHE=TARGET/'oqt-prepared'
preparation_lock = acquire_preparation_lock(CACHE)
STAMP=CACHE/'inputs.json'
inputs=[ROOT/'backend/Cargo.toml',ROOT/'backend/Cargo.lock',Path(__file__).resolve(),ROOT/'tools/oqt/preparation_lock.py']
for crate in ['oq-permissions','oqto-sandbox','oqto-hands-link','oqto-personal','oqto-personal-wire']:
    inputs.extend(sorted((ROOT/'backend/crates'/crate).rglob('*.rs')))
    inputs.append(ROOT/'backend/crates'/crate/'Cargo.toml')
policy=ROOT/'backend/crates/oqto/examples/seccomp/default.policy.toml'
inputs.append(policy)
sha=hashlib.sha256(target.encode())
for path in inputs:
    sha.update(str(path.relative_to(ROOT)).encode());sha.update(path.read_bytes())
# Cargo controls rebuild correctness; cache compiler/toolchain identity too.
sha.update(subprocess.check_output(['rustc','--version']))
source=Path(sys.argv[1]).resolve(strict=True) if len(sys.argv)>1 else Path('/tmp/oqto-pi-service-spike.PVTso2/source')
if subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()!='cdaf6c227d4718a72b59ccd02152c9614df44cfc':
    raise SystemExit('Wrong Pi source pin')
subprocess.run(['git','-C',str(source),'diff','--quiet','HEAD','--'],check=True)
sha.update(str(source).encode())
for relative in subprocess.check_output(['git','-C',str(source),'ls-files','packages/env/daemon'],text=True).splitlines():
    sha.update((source/relative).read_bytes())
key=sha.hexdigest()
binaries=[TARGET/'debug/examples/permission_carrier',TARGET/'debug/examples/oqt_hands',TARGET/'debug/examples/oqt_shim',TARGET/'debug/examples/oqt_brain',TARGET/'debug/oqt_link',TARGET/'oqt-env/debug/pi-env']
generator=TARGET/'debug/seccomp-policy-gen'
bpf=CACHE/'bpf/default-x86_64.bpf'
artifacts=[*binaries,generator,bpf] if seccomp else binaries
def identities():
    return {str(path):[path.stat().st_ino,path.stat().st_size,path.stat().st_mtime_ns] for path in artifacts if path.is_file()}
previous=json.loads(STAMP.read_text()) if STAMP.is_file() else {}
if previous.get('key')!=key or previous.get('artifacts')!=identities() or len(identities())!=len(artifacts):
    print('oqt: compiling sandbox components (first launch after an update; can take a few minutes)…', file=sys.stderr, flush=True)
    env={**os.environ,'CARGO_TARGET_DIR':str(TARGET),'CARGO_BUILD_JOBS':os.environ.get('CARGO_BUILD_JOBS','1')}
    # oqt_link: the guard-owned remote hands adapter (ADR-0086).
    builds=[['-p','oq-permissions','--example','permission_carrier','--example','oqt_hands','--example','oqt_shim','--example','oqt_brain'],['-p','oqto-hands-link','--bin','oqt_link']]
    if seccomp:
        builds.append(['-p','oqto-sandbox','--bin','seccomp-policy-gen'])
    for args in builds:
        subprocess.run(['cargo','build','--offline',*args],cwd=ROOT/'backend',env=env,check=True,timeout=600)
    subprocess.run(['cargo','build','--offline','--manifest-path',str(source/'packages/env/daemon/Cargo.toml')],env={**env,'CARGO_TARGET_DIR':str(TARGET/'oqt-env')},check=True,timeout=600)
    if seccomp:
        bpf.parent.mkdir(mode=0o700,exist_ok=True)
        subprocess.run([str(generator),'--policy',str(policy),'--out-dir',str(bpf.parent),'--arches','x86_64'],check=True,timeout=90,stdout=sys.stderr)
    temp=STAMP.with_suffix('.tmp');temp.write_text(json.dumps({'key':key,'artifacts':identities()}));temp.replace(STAMP)
print(bpf if seccomp else '')
