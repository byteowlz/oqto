"""Synthetic-only pack receiver; not security-hardened production code."""
import io,json,sys,tarfile
from pathlib import Path
root=Path(sys.argv[1]); dest=root/'cdc'
old=json.loads((root/'old.json').read_text()); seeds={}
for e in old:
    data=(dest/e['path']).read_bytes()
    for off,n,h in e['chunks']: seeds[h]=data[off:off+n]
with tarfile.open(fileobj=io.BytesIO(sys.stdin.buffer.read()),mode='r:') as t:
    entries=json.load(t.extractfile('manifest.json'))
    for m in t.getmembers():
        if m.name.startswith('chunks/'): seeds[m.name.split('/')[1]]=t.extractfile(m).read()
for e in entries:
    path=dest/e['path']; path.parent.mkdir(exist_ok=True,parents=True)
    with path.open('wb') as f:
        for off,n,h in e['chunks']: f.write(seeds[h])
print('synthetic reconstructed')
