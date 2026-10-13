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

PIN = "cdaf6c227d4718a72b59ccd02152c9614df44cfc"
repo = Path(__file__).resolve().parents[2]
source = Path(sys.argv[1]).resolve(strict=True)
workdir = Path(sys.argv[2]).resolve(strict=True)
if not workdir.is_dir():
    raise SystemExit("Work directory must exist")
if subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip() != PIN:
    raise SystemExit("Wrong Pi source pin")
subprocess.run(["git", "-C", str(source), "diff", "--quiet", "HEAD", "--"], check=True)
files = [repo / "oqt", Path(__file__).resolve(), repo / "tools/oqt/build-assets.py", repo / "tools/oqt/worker.ts", repo / "tools/oqt/launcher.ts", *sorted((repo / "tools/oqt/plugin").rglob("*"))]
files = [file for file in files if file.is_file()]
identity = hashlib.sha256(PIN.encode())
for file in files:
    identity.update(str(file.relative_to(repo)).encode()); identity.update(file.read_bytes())
binary_path = repo / "backend/target/debug/examples/permission_carrier"
meta = binary_path.stat()
binary_identity = [meta.st_ino,meta.st_size,meta.st_mtime_ns]
digest_cache = repo / "backend/target/oqt-prepared/carrier-digest.json"
saved_digest = json.loads(digest_cache.read_text()) if digest_cache.is_file() else {}
if saved_digest.get("identity") == binary_identity:
    binary_digest = saved_digest["digest"]
else:
    binary_hash = hashlib.sha256()
    with binary_path.open("rb") as binary:
        while chunk := binary.read(1024 * 1024):
            binary_hash.update(chunk)
    binary_digest = binary_hash.hexdigest()
    temporary = digest_cache.with_suffix(".tmp")
    temporary.write_text(json.dumps({"identity":binary_identity,"digest":binary_digest}))
    temporary.replace(digest_cache)
identity.update(binary_digest.encode())
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
profile = profiles / hashlib.sha256(str(workdir).encode()).hexdigest()[:12]; private(profile)
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
    shutil.copyfile(repo / "tools/oqt/worker.ts", experimental / "oqt-runtime-worker.ts")
    shutil.copyfile(repo / "tools/oqt/launcher.ts", experimental / "oqt-launcher.ts")
    plugin = snapshot / "packages/coding-agent/examples/plugins/oq-permissions"
    shutil.copytree(repo / "tools/oqt/plugin", plugin)
    tracked = [worker, experimental / "oqt-runtime-worker.ts", experimental / "oqt-launcher.ts", *[p for p in plugin.rglob("*") if p.is_file()]]
    hashes = {str(path.relative_to(snapshot)):hashlib.sha256(path.read_bytes()).hexdigest() for path in tracked}
    manifest.write_text(json.dumps({"upstream_pin":PIN,"artifact":artifact,"files":hashes},indent=2))
else:
    saved = json.loads(manifest.read_text())
    if saved["upstream_pin"] != PIN or saved["artifact"] != artifact:
        raise SystemExit("Runtime identity mismatch")
    for path, expected in saved["files"].items():
        if hashlib.sha256((snapshot/path).read_bytes()).hexdigest() != expected:
            raise SystemExit(f"Runtime source tampering: {path}")
print(json.dumps({"source":str(snapshot),"profile":str(profile),"agent":str(agent),"home":str(home),"artifact":artifact}))
