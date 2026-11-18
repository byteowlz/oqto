"""Synthetic probes only. Run: COPY_PROBE_HOST=... COPY_PROBE_REMOTE=... uv run --no-project run.py.
No service, no installer. The remote path must be an owned mktemp directory.
"""
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import subprocess
import tarfile
import time

ROOT = Path(__file__).resolve().parent
HOST = os.environ['COPY_PROBE_HOST']
os.environ['RSYNC_RSH'] = 'ssh -o ControlMaster=no -o ControlPath=none'
REMOTE = os.environ['COPY_PROBE_REMOTE']
assert REMOTE.startswith('/tmp/oqto-copy-probe.')
ENGINE = ROOT / 'engine/target/release/copy-action-probe'
os.environ['COPYFILE_DISABLE'] = '1'
ROWS = json.loads((ROOT/'results.json').read_text()) if os.environ.get('COPY_PROBE_RESUME') and (ROOT/'results.json').exists() else []
# Optional recovery of the interrupted first probe run, excluding the invalid tar trial.
ROWS = [r for r in ROWS if not r['label'].startswith('tar-')]

def cmd(args, **kw):
    if args and args[0] in ['ssh','scp']:
        args=args[:1]+['-o','ControlMaster=no','-o','ControlPath=none']+args[1:]
    return subprocess.run(args, check=True, capture_output=True, **kw)

def timed(label, args, **kw):
    start = time.perf_counter()
    p = cmd(args, **kw)
    row = dict(label=label, seconds=round(time.perf_counter()-start, 6))
    if p.stdout: row['stdout'] = p.stdout.decode(errors='replace')[-3000:]
    ROWS.append(row)
    print(json.dumps(row), flush=True)
    (ROOT / 'results.json').write_text(json.dumps(ROWS, indent=2))
    return p

def ssh(command): return cmd(['ssh', HOST, command])
def remote_rm(name):
    assert name.replace('-', '').isalnum()
    ssh(f'rm -rf {REMOTE}/{name}; mkdir -p {REMOTE}/{name}')

def generate():
    rng = random.Random(8291)
    v1 = ROOT / 'data/v1'
    if v1.exists(): return
    (v1 / 'small').mkdir(parents=True)
    (v1 / 'big').mkdir()
    for i in range(2000):
        (v1 / f'small/f{i:04d}.txt').write_bytes((f'file {i:04d}\n'.encode()*256)[:2048])
    for i in range(2):
        with (v1 / f'big/binary{i}.bin').open('wb') as f:
            for _ in range(128): f.write(rng.randbytes(1024*1024))
    (v1 / 'text.txt').write_bytes((b'{"kind":"synthetic","message":"repeated text for compression test"}\n'*300000)[:16*1024*1024])
    for p in v1.rglob('*'): os.utime(p, (1700000000,1700000000))
    v2 = ROOT / 'data/v2'
    shutil.copytree(v1, v2)
    p=v2/'big/binary0.bin'; data=p.read_bytes(); p.write_bytes(b'INSERTED'*512+data)
    with (v2/'big/binary1.bin').open('r+b') as f:
        f.seek(64*1024*1024); f.write(rng.randbytes(1024*1024))
    with (v2/'text.txt').open('ab') as f: f.write(b'new final record\n')
    for p in [v2/'big/binary0.bin',v2/'big/binary1.bin',v2/'text.txt']: os.utime(p,(1700000100,1700000100))

def manifest(version, mode):
    start=time.perf_counter(); entries=[]
    for p in sorted((ROOT/f'data/{version}').rglob('*')):
        if not p.is_file(): continue
        chunks=[]
        if p.stat().st_size <65536:
            # Whole-file hash through the pinned engine, not Python's SHA.
            r=cmd([str(ENGINE),'fixed',str(p)])
        else: r=cmd([str(ENGINE),mode,str(p)])
        for line in r.stdout.decode().splitlines():
            off,length,h=line.split('\t'); chunks.append([int(off),int(length),h])
        entries.append(dict(path=str(p.relative_to(ROOT/f'data/{version}')),size=p.stat().st_size,chunks=chunks))
    out=ROOT/f'{version}-{mode}.json'; out.write_text(json.dumps(entries,separators=(',',':')))
    return entries,time.perf_counter()-start

def make_pack(entries, known, name, version):
    out=ROOT/f'{name}.tar'; unique={}; payload=0
    for entry in entries:
        for off,n,h in entry['chunks']:
            if h not in known and h not in unique:
                unique[h]=(ROOT/f'data/{version}'/entry['path'],off,n); payload+=n
    chunksdir=ROOT/'chunks'; chunksdir.mkdir(exist_ok=True)
    with tarfile.open(out,'w') as tar:
        m=ROOT/'manifest.json'; m.write_text(json.dumps(entries)); tar.add(m,arcname='manifest.json')
        for h,(p,off,n) in unique.items():
            with p.open('rb') as f: f.seek(off); data=f.read(n)
            c=chunksdir/h; c.write_bytes(data); tar.add(c,arcname='chunks/'+h); c.unlink()
    return out,payload,len(unique)

def main():
    generate()
    print('bytes_v1',sum(p.stat().st_size for p in (ROOT/'data/v1').rglob('*') if p.is_file()),flush=True)
    # SSH startup included in every reported transfer. ControlMaster disabled consistently.
    for trial in range(1,4):
        for method in ['rsync','scp','tar']:
            if trial == 1 and any(r['label'] == f'{method}-changed-1' for r in ROWS):
                continue
            remote_rm(method)
            for phase,version in [('cold','v1'),('unchanged','v1'),('changed','v2')]:
                source=ROOT/f'data/{version}'
                if method=='rsync':
                    timed(f'{method}-{phase}-{trial}',['rsync','-a','--stats',str(source)+'/',f'{HOST}:{REMOTE}/{method}/'])
                elif method=='scp':
                    timed(f'{method}-{phase}-{trial}',['scp','-pr',str(source)+'/.',f'{HOST}:{REMOTE}/{method}/'])
                else:
                    start=time.perf_counter()
                    a=subprocess.Popen(['tar','-C',str(source),'-cf','-','.'],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
                    b=cmd(['ssh',HOST,f'tar -C {REMOTE}/{method} -xf -'],stdin=a.stdout)
                    a.stdout.close(); assert a.wait()==0
                    row=dict(label=f'tar-{phase}-{trial}',seconds=round(time.perf_counter()-start,6)); ROWS.append(row);print(json.dumps(row),flush=True)
                    (ROOT/'results.json').write_text(json.dumps(ROWS,indent=2))
            # SHA-256 tree check, path and content, not mtimes.
            verify=json.loads(ssh(f'uv run --no-project {REMOTE}/verify.py {REMOTE}/{method}').stdout)
            local=tree_hash(ROOT/'data/v2'); assert local==verify, (method, len(local), len(verify), sorted(set(verify)-set(local))[:10])
    maps={}
    for mode in ['fixed','cdc']:
        for version in ['v1','v2']:
            entries,secs=manifest(version,mode); maps[version,mode]=entries
            row=dict(label=f'manifest-{version}-{mode}',seconds=round(secs,6),chunks=sum(len(e['chunks']) for e in entries));ROWS.append(row);print(json.dumps(row),flush=True)
        known={c[2] for e in maps['v1',mode] for c in e['chunks']}
        pack,payload,n=make_pack(maps['v2',mode],known,f'delta-{mode}','v2')
        ROWS.append(dict(label=f'delta-{mode}',payload_bytes=payload,missing_chunks=n,pack_bytes=pack.stat().st_size))
    # Actual CDC seed/reconstruct pipeline. Receiver verifies complete files by SHA-256 below;
    # this prototype does NOT implement chunk auth, durable resume or sandboxing.
    remote_rm('cdc')
    cmd(['rsync','-a',str(ROOT/'data/v1')+'/',f'{HOST}:{REMOTE}/cdc/'])
    cmd(['scp',str(ROOT/'v1-cdc.json'),f'{HOST}:{REMOTE}/old.json'])
    for trial in range(1,4):
        if trial>1:
            remote_rm('cdc');cmd(['rsync','-a',str(ROOT/'data/v1')+'/',f'{HOST}:{REMOTE}/cdc/'])
        timed(f'cdc-delta-transfer-reconstruct-{trial}',['ssh',HOST,f'uv run --no-project {REMOTE}/apply.py {REMOTE}'],input=(ROOT/'delta-cdc.tar').read_bytes())
        assert json.loads(ssh(f'uv run --no-project {REMOTE}/verify.py {REMOTE}/cdc').stdout)==tree_hash(ROOT/'data/v2')
    # Bulk, four independent SSH streams. Compare same binary bytes to one stream.
    for trial in range(1,4):
        timed(f'one-stream-256MiB-{trial}',['ssh',HOST,'cat >/dev/null'],input=(ROOT/'data/v1/big/binary0.bin').read_bytes()+(ROOT/'data/v1/big/binary1.bin').read_bytes())
        start=time.perf_counter(); data=(ROOT/'data/v1/big/binary0.bin').read_bytes()[:64*1024*1024]
        with concurrent.futures.ThreadPoolExecutor(4) as pool: list(pool.map(lambda _:cmd(['ssh',HOST,'cat >/dev/null'],input=data),range(4)))
        ROWS.append(dict(label=f'four-stream-256MiB-{trial}',seconds=round(time.perf_counter()-start,6)))
    # Isolate CPU and compression from transport; no compression dictionary.
    for name in ['big/binary0.bin','text.txt']:
        p=ROOT/'data/v1'/name
        timed('blake3-'+name,[str(ENGINE),'hash',str(p)])
        for level in [1,3,9]:
            out=ROOT/'compressed.zst'; start=time.perf_counter();cmd(['zstd','-q','-f',f'-{level}',str(p),'-o',str(out)])
            ROWS.append(dict(label=f'zstd-{level}-{name}',seconds=round(time.perf_counter()-start,6),input_bytes=p.stat().st_size,output_bytes=out.stat().st_size));out.unlink()
    (ROOT/'results.json').write_text(json.dumps(ROWS,indent=2))

def tree_hash(root):
    return {str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(root.rglob('*')) if p.is_file()}
if __name__=='__main__': main()
