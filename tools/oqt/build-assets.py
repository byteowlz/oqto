#!/usr/bin/env python3
"""Warm preparation cache only; artifacts are not execution authority."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[2]
TARGET=ROOT/'backend/target'
CACHE=TARGET/'oqt-prepared'
CACHE.mkdir(mode=0o700,parents=True,exist_ok=True)
STAMP=CACHE/'inputs.json'
inputs=[ROOT/'backend/Cargo.toml',ROOT/'backend/Cargo.lock',Path(__file__).resolve()]
for crate in ['oq-permissions','oqto-sandbox']:
    inputs.extend(sorted((ROOT/'backend/crates'/crate).rglob('*.rs')))
    inputs.append(ROOT/'backend/crates'/crate/'Cargo.toml')
policy=ROOT/'backend/crates/oqto/examples/seccomp/default.policy.toml'
inputs.append(policy)
sha=hashlib.sha256()
for path in inputs:
    sha.update(str(path.relative_to(ROOT)).encode());sha.update(path.read_bytes())
# Cargo controls rebuild correctness; cache compiler/toolchain identity too.
sha.update(subprocess.check_output(['rustc','--version']))
key=sha.hexdigest()
binaries=[TARGET/'debug/examples/permission_carrier',TARGET/'debug/seccomp-policy-gen']
bpf=CACHE/'bpf/default-x86_64.bpf'
def identities():
    return {str(path):[path.stat().st_ino,path.stat().st_size,path.stat().st_mtime_ns] for path in [*binaries,bpf] if path.is_file()}
previous=json.loads(STAMP.read_text()) if STAMP.is_file() else {}
if previous.get('key')!=key or previous.get('artifacts')!=identities() or len(identities())!=3:
    env={**os.environ,'CARGO_TARGET_DIR':str(TARGET),'CARGO_BUILD_JOBS':'1'}
    for args in [['-p','oq-permissions','--example','permission_carrier'],['-p','oqto-sandbox','--bin','seccomp-policy-gen']]:
        subprocess.run(['cargo','build','--offline',*args],cwd=ROOT/'backend',env=env,check=True,timeout=90)
    bpf.parent.mkdir(mode=0o700,exist_ok=True)
    subprocess.run([str(binaries[1]),'--policy',str(policy),'--out-dir',str(bpf.parent),'--arches','x86_64'],check=True,timeout=90,stdout=sys.stderr)
    temp=STAMP.with_suffix('.tmp');temp.write_text(json.dumps({'key':key,'artifacts':identities()}));temp.replace(STAMP)
print(bpf)
