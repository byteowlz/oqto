import hashlib, json, sys
from pathlib import Path
root=Path(sys.argv[1])
print(json.dumps({str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(root.rglob('*')) if p.is_file()}))
