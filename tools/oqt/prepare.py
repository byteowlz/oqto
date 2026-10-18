#!/usr/bin/env python3
"""Create a private source-only DEMO distribution, never mutate installed Pi."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
from preparation_lock import acquire_preparation_lock

PIN = "cdaf6c227d4718a72b59ccd02152c9614df44cfc"
repo = Path(__file__).resolve().parents[2]
source = Path(sys.argv[1]).resolve(strict=True)
workdir = Path(sys.argv[2]).resolve(strict=True)
if not workdir.is_dir():
    raise SystemExit("Work directory must exist")
if subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip() != PIN:
    raise SystemExit("Wrong Pi source pin")
subprocess.run(["git", "-C", str(source), "diff", "--quiet", "HEAD", "--"], check=True)
mode = sys.argv[3] if len(sys.argv) > 3 else 'coding'
if mode not in ['demo','coding']: raise SystemExit('Unsupported runtime mode')
# Asset generation, digest-cache updates and immutable source publication share
# one process lock; a second cold launcher cannot observe a partial snapshot.
preparation_lock = acquire_preparation_lock(repo/'backend/target/oqt-prepared')
runtime_modules = [p for p in sorted((repo / 'tools/oqt').glob('*.ts')) if not p.name.endswith('.test.ts')]
runtime_modules.append(repo / 'tools/oqt/hands-proxy.mjs')
sys.path.insert(0, str(repo / 'tools/oqt'))
import source_patches
files = [repo / "oqt", Path(__file__).resolve(), repo / "tools/oqt/build-assets.py", repo / 'tools/oqt/preparation_lock.py', repo / 'tools/oqt/source_patches.py', *runtime_modules, *sorted((repo / "tools/oqt/plugin").rglob("*"))]
files = [file for file in files if file.is_file()]
identity = hashlib.sha256(PIN.encode())
for file in files:
    identity.update(str(file.relative_to(repo)).encode()); identity.update(file.read_bytes())
assets = {
    'carrier': repo/'backend/target/debug/examples/permission_carrier',
    'hands': repo/'backend/target/debug/examples/oqt_hands',
    'shim': repo/'backend/target/debug/examples/oqt_shim',
    'brain': repo/'backend/target/debug/examples/oqt_brain',
    'daemon': repo/'backend/target/oqt-env/debug/pi-env',
    'bpf': repo/'backend/target/oqt-prepared/bpf/default-x86_64.bpf',
}
digest_cache = repo/'backend/target/oqt-prepared/runtime-digests.json'
saved_digests = json.loads(digest_cache.read_text()) if digest_cache.is_file() else {}
digests = {}
for name, path in assets.items():
    meta = path.stat()
    key = [meta.st_ino,meta.st_size,meta.st_mtime_ns]
    saved = saved_digests.get(name,{})
    if saved.get('identity') == key:
        digest = saved['digest']
    else:
        sha = hashlib.sha256()
        with path.open('rb') as file:
            while chunk := file.read(1024*1024): sha.update(chunk)
        digest = sha.hexdigest()
    digests[name] = {'identity':key,'digest':digest}
    identity.update(name.encode()); identity.update(digest.encode())
temporary = digest_cache.with_suffix('.tmp'); temporary.write_text(json.dumps(digests)); temporary.replace(digest_cache)
artifact = identity.hexdigest()[:20]
# Keep Linux sockaddr_un paths below 108 bytes, including server UUID/nonces.
base = Path("/var/tmp") / f"q{os.getuid()}-{artifact[:10]}"

def private(path):
    try:
        path.mkdir(mode=0o700)
    except FileExistsError:
        meta = path.lstat()
        if not stat.S_ISDIR(meta.st_mode) or meta.st_uid != os.getuid() or meta.st_mode & 0o077:
            raise SystemExit(f"Refusing unowned/non-private directory: {path}")

private(base)
snapshot = base / "pi-source"
manifest = base / "runtime.json"
agent = base / "agent-profile"; private(agent)
home = base / "home"; private(home)
profiles = base / "p"; private(profiles)
profile = profiles / hashlib.sha256((str(workdir)+':'+mode).encode()).hexdigest()[:12]; private(profile)
if len(os.fsencode(profile / "control-00000000-0000-0000-0000-000000000000.sock")) >= 108:
    raise SystemExit("Profile exceeds Linux Unix-socket path limit")
binding_file = profile / "work-directory.json"
if binding_file.exists():
    if json.loads(binding_file.read_text()) != {"work_directory":str(workdir)}:
        raise SystemExit("Work-directory profile collision; refusing reuse")
else:
    with binding_file.open("x") as output:
        json.dump({"work_directory":str(workdir)},output)
if not manifest.exists():
    print('oqt: preparing a private runtime for this version…', file=sys.stderr, flush=True)
    if snapshot.exists():
        raise SystemExit("Incomplete source snapshot; inspect it rather than overwriting")
    snapshot.mkdir(mode=0o700)
    archive = base / "source.tar"
    with archive.open("xb") as output:
        subprocess.run(["git", "-C", str(source), "archive", PIN], stdout=output, check=True)
    subprocess.run(["tar", "-xf", str(archive), "-C", str(snapshot)], check=True)
    archive.unlink()
    (snapshot / "node_modules").symlink_to(source / "node_modules", target_is_directory=True)
    data = source / "packages/ai/src/providers/data"
    if data.exists():
        destination = snapshot / "packages/ai/src/providers/data"
        shutil.copytree(data, destination, dirs_exist_ok=True)
    experimental = snapshot / "packages/coding-agent/src/experimental"
    worker = experimental / "session-worker.ts"
    original = "return runSessionWorkerWithHarness(args, createCodingAgentHarness);"
    text = worker.read_text()
    if text.count(original) != 1:
        raise SystemExit("Unknown upstream worker entry; refusing stock-tools fallback")
    # Reuse the public Harness factory seam. This private derivative replaces
    # only application composition, not Durable/protocol/client machinery.
    worker.write_text(text.replace(original, 'return import("./oqt-runtime-worker.ts").then(({createOqtHarness}) => runSessionWorkerWithHarness(args, createOqtHarness));'))
    patched = source_patches.apply(snapshot)
    shutil.copyfile(repo / "tools/oqt/worker.ts", experimental / "oqt-runtime-worker.ts")
    shutil.copyfile(repo / "tools/oqt/launcher.ts", experimental / "oqt-launcher.ts")
    copied_modules = []
    for module in runtime_modules:
        if module.name in ['worker.ts','launcher.ts']: continue
        target = experimental/module.name
        shutil.copyfile(module,target); copied_modules.append(target)
    plugin = snapshot / "packages/coding-agent/examples/plugins/oq-permissions"
    shutil.copytree(repo / "tools/oqt/plugin", plugin)
    tracked = [worker, *dict.fromkeys(patched), experimental / "oqt-runtime-worker.ts", experimental / "oqt-launcher.ts", *copied_modules, *[p for p in plugin.rglob("*") if p.is_file()]]
    hashes = {str(path.relative_to(snapshot)):hashlib.sha256(path.read_bytes()).hexdigest() for path in tracked}
    manifest.write_text(json.dumps({"upstream_pin":PIN,"artifact":artifact,"files":hashes},indent=2))
else:
    saved = json.loads(manifest.read_text())
    if saved["upstream_pin"] != PIN or saved["artifact"] != artifact:
        raise SystemExit("Runtime identity mismatch")
    for path, expected in saved["files"].items():
        if hashlib.sha256((snapshot/path).read_bytes()).hexdigest() != expected:
            raise SystemExit(f"Runtime source tampering: {path}")
# Stage trusted execution assets outside the workspace/private brain root.
binroot = Path('/var/tmp')/f'oqbin-{os.getuid()}-{artifact[:10]}'; private(binroot)
for name, original in assets.items():
    target = binroot/name
    if not target.exists():
        pending = target.with_suffix('.tmp')
        with original.open('rb') as file, pending.open('xb') as output:
            shutil.copyfileobj(file,output)
        sha = hashlib.sha256()
        with pending.open('rb') as file:
            while chunk := file.read(1024*1024): sha.update(chunk)
        if sha.hexdigest()!=digests[name]['digest']: raise SystemExit('Staged binary identity mismatch')
        pending.chmod(0o500); pending.replace(target)
    meta = target.lstat()
    if not stat.S_ISREG(meta.st_mode) or meta.st_uid!=os.getuid() or meta.st_mode & 0o077:
        raise SystemExit('Unowned/non-private execution asset')
if workdir == binroot or binroot.is_relative_to(workdir):
    raise SystemExit('Workspace includes trusted execution assets; choose a narrower directory')
print(json.dumps({"source":str(snapshot),"profile":str(profile),"agent":str(agent),"home":str(home),"artifact":artifact,'runtime_root':str(base),**{name:str(binroot/name) for name in assets}}))
