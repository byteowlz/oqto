"""Follow-up probes after run.py has completed; same environment variables."""
import hashlib, json, os, statistics, subprocess, time
from pathlib import Path
ROOT=Path(__file__).resolve().parent
HOST=os.environ['COPY_PROBE_HOST']; REMOTE=os.environ['COPY_PROBE_REMOTE']
assert REMOTE.startswith('/tmp/oqto-copy-probe.')
ROWS=[]
os.environ['RSYNC_RSH']='ssh -o ControlMaster=no -o ControlPath=none'
def run(args,**kw):
    if args[0] in ['ssh','scp']: args=args[:1]+['-o','ControlMaster=no','-o','ControlPath=none']+args[1:]
    return subprocess.run(args,check=True,capture_output=True,**kw)
def timed(label,args,**kw):
    start=time.perf_counter();p=run(args,**kw)
    row=dict(label=label,seconds=round(time.perf_counter()-start,6),stdout=p.stdout.decode(errors='replace')[-3000:],stderr=p.stderr.decode(errors='replace')[-3000:])
    ROWS.append(row);(ROOT/'followup.json').write_text(json.dumps(ROWS,indent=2));print(json.dumps(row),flush=True)
    return p
for trial in range(1,4): timed(f'rsync-strict-unchanged-{trial}',['rsync','-ac','--stats',str(ROOT/'data/v2')+'/',f'{HOST}:{REMOTE}/rsync/'])
# Verify reverse copies in addition to measuring. One trial, not a distribution.
for method in ['rsync','scp']:
    dest=ROOT/f'data/reverse-{method}';dest.mkdir(exist_ok=True)
    if method=='rsync':args=['rsync','-a','--stats',f'{HOST}:{REMOTE}/rsync/',str(dest)+'/']
    else:args=['scp','-pr',f'{HOST}:{REMOTE}/scp/.',str(dest)+'/']
    timed(f'reverse-{method}-cold',args)
    def hashes(root):return {str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in root.rglob('*') if p.is_file()}
    assert hashes(dest)==hashes(ROOT/'data/v2')
    ROWS.append(dict(label=f'reverse-{method}-sha256',files=2003,passed=True))
# Local CPU timings include file load; engine stderr separately records in-memory work.
engine=ROOT/'engine/target/release/copy-action-probe'
for mode in ['hash','fixed','cdc']:
    for trial in range(1,4): timed(f'cpu-{mode}-{trial}',[str(engine),mode,str(ROOT/'data/v1/big/binary0.bin')])
timed('shortcuts-mac',['uv','run','--no-project',str(ROOT/'shortcuts.py'),str(ROOT/'data/v1/big/binary0.bin')])
run(['scp',str(ROOT/'shortcuts.py'),f'{HOST}:{REMOTE}/shortcuts.py'])
timed('shortcuts-linux',['ssh',HOST,f'uv run --no-project {REMOTE}/shortcuts.py {REMOTE}/rsync/big/binary0.bin'])
# Persistent manifest arithmetic, not a restart/durability test.
for mode in ['fixed','cdc']:
    old=json.loads((ROOT/f'v1-{mode}.json').read_text());new=json.loads((ROOT/f'v2-{mode}.json').read_text())
    oldhash={c[2] for e in old for c in e['chunks']}
    newchunks={c[2]:c[1] for e in new for c in e['chunks']}
    missing={h:n for h,n in newchunks.items() if h not in oldhash}
    half=set(list(missing)[:len(missing)//2]); after=oldhash|half
    ROWS.append(dict(label=f'persisted-set-{mode}',unchanged_missing_bytes=0,changed_missing_bytes=sum(missing.values()),after_half_missing_bytes=sum(n for h,n in missing.items() if h not in after),renamed_only_missing_bytes=0))
(ROOT/'followup.json').write_text(json.dumps(ROWS,indent=2))
