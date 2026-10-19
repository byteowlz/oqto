#!/usr/bin/env python3
"""Publish self-contained oqt execution artifacts; never read owner settings.

Interface for installed launch/preparation (which are supplied by the caller)::

  install-runtime.py install --prepared-json preparation.json --launcher oqt-installed \
      [--runtime-file tools/oqt/prepare-installed.py=/path/to/prepare-installed.py]
  install-runtime.py verify --runtime ~/.local/share/oqt/runtimes/<sha256>
  install-runtime.py activate --runtime ... --launcher-destination ~/.local/bin/oqt

Install does NOT activate unless --activate is supplied. preparation.json is the
existing prepare.py output, generated with disposable work/owner directories.
Only source and the six native assets are consumed, not its profiles/home.
Verification returns runtime-local paths with prepare.py-compatible asset keys.
Published trees are owner-readable/executable, not writable; hashes and modes
cover every entry. This is same-uid tamper detection, not an OS immutability or
signature boundary. The expected identity is the runtime directory basename.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import sys
import tempfile

ASSETS = ('carrier', 'hands', 'shim', 'brain', 'daemon', 'bpf')
SCHEMA = 1


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def digest(path):
    sha = hashlib.sha256()
    with path.open('rb') as stream:
        while chunk := stream.read(1024 * 1024):
            sha.update(chunk)
    return sha.hexdigest()


def private(path, strict=True):
    """Create/check every path component without following directory symlinks.

    Ancestors may be shared (e.g. /tmp), but not writable except sticky dirs.
    Private managed directories require 0700. A shared user namespace may
    retain its owned non-group/world-writable mode (e.g. existing 0755).
    """
    path = Path(os.path.abspath(path))
    for component in [*reversed(path.parents), path]:
        try:
            meta = component.lstat()
        except FileNotFoundError:
            component.mkdir(mode=0o700)
            meta = component.lstat()
        if not stat.S_ISDIR(meta.st_mode):
            raise ValueError(f'Not a real directory: {component}')
        if meta.st_uid not in (0, os.getuid()):
            raise ValueError(f'Untrusted directory owner: {component}')
        if meta.st_mode & 0o022 and not meta.st_mode & stat.S_ISVTX:
            raise ValueError(f'Writable ancestor: {component}')
    meta = path.lstat()
    if meta.st_uid != os.getuid() or (strict and stat.S_IMODE(meta.st_mode) != 0o700) or meta.st_mode & 0o022:
        raise ValueError(f'Not an owned safe directory: {path}')
    return path


def regular(path):
    meta = path.lstat()
    if not stat.S_ISREG(meta.st_mode):
        raise ValueError(f'Not a regular file: {path}')
    return meta


def copy_tree(source, destination, snapshot, upstream, ancestors=()):
    """Dereference dependencies; workspace links use the PATCHED snapshot.

    Never carry executable links back to the checkout. Reject cycles, dangling
    links, sockets/devices and special files instead of silently omitting them.
    Build and VCS caches are not execution dependencies and are not bundled.
    """
    source = source.resolve(strict=True)
    if upstream and source.is_relative_to(upstream) and not source.is_relative_to(upstream / 'node_modules'):
        relative = source.relative_to(upstream)
        derivative = snapshot / relative
        if derivative.exists():
            source = derivative.resolve(strict=True)
    meta = source.stat()
    if stat.S_ISDIR(meta.st_mode):
        identity = (meta.st_dev, meta.st_ino)
        if identity in ancestors:
            raise ValueError(f'Dependency symlink cycle: {source}')
        destination.mkdir(mode=0o700)
        for child in sorted(source.iterdir()):
            if child.name in ('.git', 'target', '__pycache__'):
                continue
            copy_tree(child, destination / child.name, snapshot, upstream, (*ancestors, identity))
        destination.chmod(0o500)
    elif stat.S_ISREG(meta.st_mode):
        with source.open('rb') as stream, destination.open('xb') as output:
            shutil.copyfileobj(stream, output)
        destination.chmod(0o500 if meta.st_mode & 0o111 else 0o400)
    else:
        raise ValueError(f'Special dependency file: {source}')


def inventory(root):
    records = {}
    def visit(directory):
        for child in sorted(directory.iterdir()):
            relative = child.relative_to(root).as_posix()
            if relative == 'manifest.json':
                continue
            meta = child.lstat()
            if meta.st_uid != os.getuid():
                raise ValueError(f'Unexpected owner: {child}')
            mode = stat.S_IMODE(meta.st_mode)
            if stat.S_ISDIR(meta.st_mode):
                if mode != 0o500:
                    raise ValueError(f'Writable/non-private runtime directory: {child}')
                records[relative] = {'type': 'directory', 'mode': mode}
                visit(child)
            elif stat.S_ISREG(meta.st_mode):
                if mode not in (0o400, 0o500) or meta.st_nlink != 1:
                    raise ValueError(f'Unsafe runtime file mode/link: {child}')
                records[relative] = {'type': 'file', 'mode': mode, 'sha256': digest(child)}
            else:
                raise ValueError(f'Runtime link/special file: {child}')
    visit(root)
    return records


def runtime_paths(root):
    return {'runtime_root': str(root), 'source': str(root / 'pi-source'),
            'node': str(root / 'bin/node'),
            **{name: str(root / 'bin' / name) for name in ASSETS}}


def result(root, identity):
    return {'artifact': identity, 'launcher': str(root / 'oqt'), **runtime_paths(root)}


def relative_runtime_paths():
    return {'runtime_root': '.', 'source': 'pi-source', 'node': 'bin/node',
            **{name: 'bin/' + name for name in ASSETS}}


def manifest_identity(manifest):
    return hashlib.sha256(canonical(manifest)).hexdigest()


def verify(runtime):
    root = Path(os.path.abspath(runtime))
    if not re.fullmatch('[0-9a-f]{64}', root.name):
        raise ValueError('Runtime must be named by its full SHA-256 identity')
    # Managed parent is private; runtime itself is deliberately read-only.
    private(root.parent)
    meta = root.lstat()
    if not stat.S_ISDIR(meta.st_mode) or meta.st_uid != os.getuid() or stat.S_IMODE(meta.st_mode) != 0o500:
        raise ValueError('Unsafe runtime root')
    manifest = root / 'manifest.json'
    metadata = regular(manifest)
    if stat.S_IMODE(metadata.st_mode) != 0o400 or metadata.st_uid != os.getuid() or metadata.st_nlink != 1:
        raise ValueError('Unsafe runtime manifest')
    saved = json.loads(manifest.read_bytes())
    if set(saved) != {'schema', 'files'} or saved['schema'] != SCHEMA:
        raise ValueError('Unsupported runtime manifest')
    if manifest_identity(saved) != root.name:
        raise ValueError('Manifest identity mismatch')
    if inventory(root) != saved['files']:
        raise ValueError('Runtime tampering: content, mode, or entry set mismatch')
    if json.loads((root / 'runtime.json').read_bytes()) != relative_runtime_paths():
        raise ValueError('Invalid runtime path metadata')
    validate_layout(saved['files'])
    return result(root, root.name)


def validate_layout(files):
    for required in ['oqt', 'verifier.py', 'runtime.json', 'launch.py', 'bin/node',
                     *['bin/' + key for key in ASSETS]]:
        if files.get(required, {}).get('type') != 'file':
            raise ValueError(f'Missing runtime file: {required}')
    for required in ('pi-source', 'pi-source/node_modules', 'bin'):
        if files.get(required, {}).get('type') != 'directory':
            raise ValueError(f'Missing runtime directory: {required}')


# Public API used by installed-launch.py; verification never prepares profiles.
verify_runtime = verify


def remove_stage(path):
    if not path.exists():
        return
    for directory, _, _ in os.walk(path):
        Path(directory).chmod(0o700)
    shutil.rmtree(path)


def install(prepared, launcher, data_root, node, extra=()):
    data_root = private(data_root, strict=False)
    runtimes = private(data_root / 'runtimes')
    snapshot = Path(prepared['source']).resolve(strict=True)
    # prepare.py intentionally links node_modules to the upstream checkout.
    modules = snapshot / 'node_modules'
    upstream = modules.resolve(strict=True).parent if modules.is_symlink() else None
    stage = Path(tempfile.mkdtemp(prefix='.staging-', dir=runtimes))
    try:
        copy_tree(snapshot, stage / 'pi-source', snapshot, upstream)
        native = stage / 'bin'
        native.mkdir(mode=0o700)
        for name in ASSETS:
            source = Path(prepared[name])
            regular(source)
            copy_tree(source, native / name, snapshot, None)
            (native / name).chmod(0o400 if name == 'bpf' else 0o500)
        copy_tree(Path(node), native / 'node', snapshot, None)
        (native / 'node').chmod(0o500)
        native.chmod(0o500)
        copy_tree(Path(launcher), stage / 'oqt', snapshot, None)
        (stage / 'oqt').chmod(0o500)
        copy_tree(Path(__file__), stage / 'verifier.py', snapshot, None)
        # Keep the descriptive installer name for launchers already using it;
        # both are independent regular files in the same hashed snapshot.
        copy_tree(Path(__file__), stage / 'install-runtime.py', snapshot, None)
        for specification in extra:
            relative, source = specification.split('=', 1)
            relative = Path(relative)
            if relative.is_absolute() or not relative.parts or '..' in relative.parts:
                raise ValueError('Runtime file must use a safe relative path')
            if relative.parts[0] in ('pi-source', 'bin', 'oqt', 'runtime.json', 'manifest.json', 'verifier.py', 'install-runtime.py'):
                raise ValueError('Runtime file collides with a reserved entry')
            destination = stage / relative
            destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            copy_tree(Path(source), destination, snapshot, None)
        # Normalize newly created support directories before hashing.
        for directory, _, _ in os.walk(stage):
            if Path(directory) != stage:
                Path(directory).chmod(0o500)
        with (stage / 'runtime.json').open('xb') as output:
            output.write(canonical(relative_runtime_paths()))
        (stage / 'runtime.json').chmod(0o400)
        manifest = {'schema': SCHEMA, 'files': inventory(stage)}
        validate_layout(manifest['files'])
        identity = manifest_identity(manifest)
        destination = runtimes / identity
        with (stage / 'manifest.json').open('xb') as output:
            output.write(canonical(manifest))
        (stage / 'manifest.json').chmod(0o400)
        stage.chmod(0o500)
        lock = activation_lock(data_root)
        try:
            if destination.exists() or destination.is_symlink():
                verify(destination)
            else:
                stage.rename(destination)
            return verify(destination)
        finally:
            os.close(lock)
    finally:
        remove_stage(stage)


def activation_lock(data_root):
    path = data_root / '.install.lock'
    descriptor = os.open(path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    meta = os.fstat(descriptor)
    if not stat.S_ISREG(meta.st_mode) or meta.st_uid != os.getuid() or stat.S_IMODE(meta.st_mode) != 0o600 or meta.st_nlink != 1:
        os.close(descriptor)
        raise ValueError('Unsafe installer lock')
    fcntl.flock(descriptor, fcntl.LOCK_EX)
    return descriptor


def backup_launcher(destination, data_root):
    """Snapshot each unmanaged replacement, including dereferenced link bytes.

    A new private directory is used every time: a previous backup must never
    suppress capture of later user edits. Relative link text and original path
    are recorded for restoration, independently of the immutable byte snapshot.
    """
    meta = destination.lstat()
    if meta.st_uid != os.getuid() or not (stat.S_ISREG(meta.st_mode) or stat.S_ISLNK(meta.st_mode)):
        raise ValueError('Unsafe installed launcher')
    backups = private(data_root / 'launcher-backups')
    backup = Path(tempfile.mkdtemp(prefix='launcher-', dir=backups))
    try:
        resolved = destination.resolve(strict=True)
        target_meta = regular(resolved)
        record = {'path': str(destination), 'mode': stat.S_IMODE(meta.st_mode),
                  'target_mode': stat.S_IMODE(target_meta.st_mode)}
        if destination.is_symlink():
            record['link_target'] = os.readlink(destination)
            (backup / 'launcher.symlink').symlink_to(record['link_target'])
        with resolved.open('rb') as original, (backup / 'launcher.bytes').open('xb') as output:
            shutil.copyfileobj(original, output)
        (backup / 'launcher.bytes').chmod(0o400)
        record['sha256'] = digest(backup / 'launcher.bytes')
        with (backup / 'metadata.json').open('xb') as output:
            output.write(canonical(record))
        (backup / 'metadata.json').chmod(0o400)
        return backup
    except BaseException:
        shutil.rmtree(backup)
        raise


def activate(runtime, launcher_destination=None):
    runtime = Path(os.path.abspath(runtime))
    data_root = private(runtime.parent.parent, strict=False)
    if runtime.parent != data_root / 'runtimes':
        raise ValueError('Runtime is not in the managed runtimes directory')
    lock = activation_lock(data_root)
    try:
        info = verify(runtime)
        current = data_root / 'current'
        if current.exists() or current.is_symlink():
            if not current.is_symlink() or not re.fullmatch(r'runtimes/[0-9a-f]{64}', os.readlink(current)):
                raise ValueError('Unsafe current link')
        previous_current = os.readlink(current) if current.is_symlink() else None
        # Validate and back up launcher before changing either public pointer.
        pending_launcher = None
        if launcher_destination is not None:
            destination = Path(os.path.abspath(launcher_destination))
            # ~/.local/bin is normally 0755: ancestors must be owned and not
            # group/world-writable, but do not change user directory modes.
            for ancestor in [destination.parent, *destination.parent.parents]:
                meta = ancestor.lstat()
                if not stat.S_ISDIR(meta.st_mode) or meta.st_uid not in (0, os.getuid()) or (meta.st_mode & 0o022 and not meta.st_mode & stat.S_ISVTX):
                    raise ValueError(f'Unsafe launcher directory: {ancestor}')
            managed_target = str(data_root / 'current/oqt')
            managed = destination.is_symlink() and os.readlink(destination) == managed_target
            if managed:
                if destination.lstat().st_uid != os.getuid():
                    raise ValueError('Unowned managed launcher')
            else:
                if destination.exists() or destination.is_symlink():
                    backup_launcher(destination, data_root)
                descriptor, temporary = tempfile.mkstemp(prefix='.oqt-launcher-', dir=destination.parent)
                pending_launcher = Path(temporary)
                try:
                    os.close(descriptor)
                    pending_launcher.unlink()
                    pending_launcher.symlink_to(managed_target)
                except BaseException:
                    pending_launcher.unlink(missing_ok=True)
                    raise
        temporary = data_root / ('.current-' + os.urandom(12).hex())
        try:
            temporary.symlink_to('runtimes/' + runtime.name)
            # Publish a valid first pointer BEFORE replacing an unmanaged
            # command. A crash here leaves the old launcher intact and usable.
            temporary.replace(current)
            if pending_launcher:
                try:
                    pending_launcher.replace(destination)
                except BaseException:
                    # A failed replacement must preserve both previous entry
                    # points. Managed launchers need no replacement at all.
                    if previous_current is None:
                        current.unlink()
                    else:
                        temporary.symlink_to(previous_current)
                        temporary.replace(current)
                    raise
        finally:
            temporary.unlink(missing_ok=True)
            if pending_launcher:
                pending_launcher.unlink(missing_ok=True)
        return info
    finally:
        os.close(lock)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    install_parser = commands.add_parser('install')
    install_parser.add_argument('--prepared-json', required=True, type=Path)
    install_parser.add_argument('--launcher', required=True, type=Path)
    install_parser.add_argument('--node', required=True, type=Path)
    install_parser.add_argument('--runtime-file', action='append', default=[])
    install_parser.add_argument('--data-root', type=Path, default=Path.home() / '.local/share/oqt')
    install_parser.add_argument('--activate', action='store_true')
    install_parser.add_argument('--launcher-destination', type=Path)
    for name in ('verify', 'activate'):
        command = commands.add_parser(name)
        command.add_argument('--runtime', required=True, type=Path)
        if name == 'activate':
            command.add_argument('--launcher-destination', type=Path)
    args = parser.parse_args()
    if args.command == 'install':
        if args.launcher_destination and not args.activate:
            parser.error('--launcher-destination requires --activate')
        info = install(json.loads(args.prepared_json.read_bytes()), args.launcher, args.data_root, args.node, args.runtime_file)
        if args.activate:
            info = activate(Path(info['runtime_root']), args.launcher_destination)
    elif args.command == 'verify':
        info = verify(args.runtime)
    else:
        info = activate(args.runtime, args.launcher_destination)
    print(json.dumps(info, sort_keys=True))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, RuntimeError) as error:
        print(f'oqt installer: {error}', file=sys.stderr)
        sys.exit(1)
