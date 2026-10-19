"""Private persistent installed-runtime state with short, checked IPC aliases."""
import json
import hashlib
import os
from pathlib import Path
import stat
import re


def private_directory(path):
    path = Path(path)
    try:
        path.mkdir(mode=0o700)
    except FileNotFoundError:
        private_directory(path.parent)
        path.mkdir(mode=0o700)
    except FileExistsError:
        pass
    meta = path.lstat()
    if not stat.S_ISDIR(meta.st_mode) or meta.st_uid != os.getuid() or meta.st_mode & 0o077:
        raise RuntimeError(f"State directory must be private, owned and not a symlink: {path}")
    return path


def ipc_alias(state_home, artifact, ipc_parent=Path('/var/tmp')):
    target = Path(state_home).resolve()/'oqt/runtimes'/artifact
    key = hashlib.sha256(os.fsencode(target)).hexdigest()[:10]
    return Path(ipc_parent)/f'qi{os.getuid()}-{key}'


def prepare_state(runtime, artifact, workdir, mode, owner, state_home=None, ipc_parent=Path('/var/tmp')):
    from profile_namespace import workspace_profile_key
    from preparation_lock import acquire_preparation_lock

    if not re.fullmatch(r'[a-f0-9]{20,64}', artifact):
        raise RuntimeError('Invalid installed artifact identity')
    root = Path(state_home or os.environ.get('XDG_STATE_HOME', str(Path.home()/'.local/state')))
    if not root.is_absolute():
        raise RuntimeError('XDG_STATE_HOME must be absolute')
    # The XDG parent is user-owned shared state, not necessarily mode 0700.
    for component in [*reversed(root.parents), root]:
        try:
            meta = component.lstat()
        except FileNotFoundError:
            component.mkdir(mode=0o700)
            meta = component.lstat()
        if not stat.S_ISDIR(meta.st_mode) or meta.st_uid not in (0, os.getuid()) or (meta.st_mode & 0o022 and not meta.st_mode & stat.S_ISVTX):
            raise RuntimeError('Unsafe state ancestor')
    if root.stat().st_uid != os.getuid() or root.stat().st_mode & 0o022:
        raise RuntimeError('Unsafe state parent')
    store = root/'oqt'
    store.mkdir(mode=0o700, exist_ok=True)
    meta = store.lstat()
    if not stat.S_ISDIR(meta.st_mode) or meta.st_uid != os.getuid() or meta.st_mode & 0o022:
        raise RuntimeError('Unsafe state namespace')
    versions = private_directory(store/'runtimes')
    state = private_directory(versions/artifact)
    parent_meta = Path(ipc_parent).lstat()
    if not stat.S_ISDIR(parent_meta.st_mode) or parent_meta.st_uid not in (0, os.getuid()) or (parent_meta.st_mode & 0o022 and not parent_meta.st_mode & stat.S_ISVTX):
        raise RuntimeError('Unsafe IPC parent')
    lock = acquire_preparation_lock(state/'locks')
    try:
        alias = ipc_alias(root, artifact, ipc_parent)
        try:
            alias.symlink_to(state, target_is_directory=True)
        except FileExistsError:
            meta = alias.lstat()
            if not stat.S_ISLNK(meta.st_mode) or meta.st_uid != os.getuid() or os.readlink(alias) != str(state):
                raise RuntimeError('Unsafe or colliding IPC alias')
        agent = private_directory(state/'agent-profile')
        home = private_directory(state/'home')
        profiles = private_directory(state/'p')
        key = workspace_profile_key(workdir, mode, owner)
        profile = private_directory(profiles/key)
        binding = profile/'work-directory.json'
        wanted = {'work_directory': str(workdir)}
        flags = os.O_RDONLY | os.O_NOFOLLOW
        try:
            fd = os.open(binding, flags)
        except FileNotFoundError:
            fd = os.open(binding, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            with os.fdopen(fd, 'w') as output:
                json.dump(wanted, output)
        else:
            with os.fdopen(fd) as value:
                meta = os.fstat(value.fileno())
                if not stat.S_ISREG(meta.st_mode) or meta.st_uid != os.getuid() or meta.st_mode & 0o077 or json.load(value) != wanted:
                    raise RuntimeError('Invalid work-directory binding')
        short_profile = alias/'p'/key
        if len(os.fsencode(short_profile/'control-00000000-0000-0000-0000-000000000000.sock')) >= 108:
            raise RuntimeError('Installed profile exceeds Unix socket limit')
        return {'runtime_root': str(alias), 'profile': str(short_profile),
                'agent': str(alias/'agent-profile'), 'home': str(alias/'home')}
    finally:
        os.close(lock)
