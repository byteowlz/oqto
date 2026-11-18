"""Local shortcut probe only; reports filesystem operations, no durable fsync claim."""
import hashlib,json,os,platform,shutil,subprocess,sys,time
from pathlib import Path
source=Path(sys.argv[1]);dest=source.with_name('shortcut-output.bin')
expected=hashlib.sha256(source.read_bytes()).digest();rows=[]
for trial in range(1,4):
    start=time.perf_counter();shutil.copyfile(source,dest)
    rows.append(dict(method='shutil-copyfile',trial=trial,seconds=time.perf_counter()-start,bytes=source.stat().st_size))
    assert hashlib.sha256(dest.read_bytes()).digest()==expected;dest.unlink()
    start=time.perf_counter()
    if platform.system()=='Darwin':
        p=subprocess.run(['cp','-c',str(source),str(dest)],capture_output=True)
        rows.append(dict(method='cp-clone',trial=trial,seconds=time.perf_counter()-start,exit_code=p.returncode))
        if p.returncode==0:assert hashlib.sha256(dest.read_bytes()).digest()==expected;dest.unlink()
    else:
        try:
            with source.open('rb') as src,dest.open('wb') as dst:
                while os.copy_file_range(src.fileno(),dst.fileno(),1024*1024):pass
            rows.append(dict(method='copy-file-range',trial=trial,seconds=time.perf_counter()-start))
            assert hashlib.sha256(dest.read_bytes()).digest()==expected;dest.unlink()
        except OSError as e:
            rows.append(dict(method='copy-file-range',trial=trial,errno=e.errno));dest.unlink(missing_ok=True)
        start=time.perf_counter();p=subprocess.run(['cp','--reflink=always',str(source),str(dest)],capture_output=True)
        rows.append(dict(method='cp-reflink',trial=trial,seconds=time.perf_counter()-start,exit_code=p.returncode))
        dest.unlink(missing_ok=True)
print(json.dumps(rows))
