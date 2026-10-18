"""Serialize source-cache builders, not interactive runtime processes."""
import fcntl
import os
from pathlib import Path
import stat


def acquire_preparation_lock(directory: Path) -> int:
    """Return an owned lock FD; close it on exit (never inherited by exec).

    Both asset building and source publication use this same lock. Unknown,
    public, symlinked or foreign-owned cache/lock entries fail closed.
    """
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    metadata = directory.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise RuntimeError('Preparation cache must be private and owned')
    descriptor = os.open(directory / '.preparation.lock', os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
            raise RuntimeError('Preparation lock must be private and owned')
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise
