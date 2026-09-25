#!/usr/bin/env python3
"""Extract only oqto-setup from a verified full Linux release bundle.

Bootstrap boundary, NOT an alternative activation engine: after this completes,
run the extracted oqto-setup with the same artifact and independent checksum.
Never extract arbitrary archive paths or execute bytes before verification.
"""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import re
import stat
import sys
import tarfile

MAX_ARCHIVE_BYTES = 2_500_000_000
MAX_MEMBERS = 100_000
MAX_EXPANDED_BYTES = 2 * 1024 * 1024 * 1024
MAX_INSTALLER_BYTES = 128 * 1024 * 1024
MAX_TRAILING_BYTES = 1024 * 1024
REQUIRED_BINS = {
    "oqto", "oqtoctl", "oqto-setup", "oqto-runner", "oqto-files",
    "oqto-sandbox", "oqto-usermgr", "pi-bridge",
}
RELEASE_NAME = re.compile(r"oqto-v[0-9][A-Za-z0-9._+-]*-(?:x86_64|aarch64)-unknown-linux-gnu")


class InvalidBundle(ValueError):
    """Fail closed without exposing artifact contents or local credentials."""


def _checksum(archive: Path, checksum_file: Path) -> str:
    with checksum_file.open("rb") as source:
        body = source.read(8193)
    if len(body) > 8192:
        raise InvalidBundle("checksum metadata exceeds 8 KiB")
    try:
        lines = body.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        raise InvalidBundle("checksum metadata is not UTF-8") from error
    if len(lines) != 1:
        raise InvalidBundle("checksum must contain exactly one artifact line")
    fields = lines[0].split()
    if len(fields) != 2 or not re.fullmatch(r"[0-9a-fA-F]{64}", fields[0]):
        raise InvalidBundle("invalid SHA-256 checksum line")
    if Path(fields[1].lstrip("*")).name != archive.name:
        raise InvalidBundle("checksum does not name this artifact")
    return fields[0].lower()


def _verify_archive(archive: Path, expected_hash: str) -> None:
    metadata = archive.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > MAX_ARCHIVE_BYTES:
        raise InvalidBundle("archive is not a bounded regular file")
    digest = hashlib.sha256()
    with archive.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    if digest.hexdigest() != expected_hash:
        raise InvalidBundle("release artifact SHA-256 mismatch")


def _member_name(name: str, root: str, is_dir: bool) -> str:
    if name.startswith("/") or "\\" in name or "\x00" in name or len(name) > 4096:
        raise InvalidBundle("unsafe release member path")
    components = name.split("/")
    if is_dir and components[-1] == "":
        components.pop()
    if not components or components[0] != root or any(
        part in {"", ".", ".."} for part in components
    ):
        raise InvalidBundle("release member escapes its canonical root")
    return "/".join(components)


def _extract_verified(archive: Path, output: Path) -> None:
    if not archive.name.endswith(".tar.gz"):
        raise InvalidBundle("release artifact must end with .tar.gz")
    root = archive.name[: -len(".tar.gz")]
    if not RELEASE_NAME.fullmatch(root):
        raise InvalidBundle("expected a named full Linux release artifact")
    required = {f"{root}/immutable/bin/{name}" for name in REQUIRED_BINS}
    installer_name = f"{root}/immutable/bin/oqto-setup"
    manifest_name = f"{root}/manifest.toml"
    frontend_name = f"{root}/immutable/frontend/dist/index.html"
    seen: set[str] = set()
    total = 0
    installer: tarfile.TarInfo | None = None
    try:
        with tarfile.open(archive, "r:gz") as contents:
            for member in contents:
                if len(seen) >= MAX_MEMBERS:
                    raise InvalidBundle("release has too many members")
                if not (member.isfile() or member.isdir()):
                    raise InvalidBundle("release contains a link or special entry")
                name = _member_name(member.name, root, member.isdir())
                if name in seen:
                    raise InvalidBundle("release has duplicate members")
                seen.add(name)
                if member.mode & 0o7000:
                    raise InvalidBundle("release contains privileged mode bits")
                if member.size < 0 or member.size > MAX_EXPANDED_BYTES - total:
                    raise InvalidBundle("release expanded content exceeds 2 GiB")
                total += member.size
                if name.startswith(f"{root}/immutable/bin/") and name not in required:
                    raise InvalidBundle("release contains an unexpected bin entry")
                if name in required and (not member.isfile() or not member.mode & 0o111):
                    raise InvalidBundle("required binary is not a regular executable")
                if name == installer_name:
                    if member.size == 0 or member.size > MAX_INSTALLER_BYTES:
                        raise InvalidBundle("installer size is invalid")
                    installer = member
                if name in {manifest_name, frontend_name} and (
                    not member.isfile() or member.size == 0 or member.size > 1024 * 1024
                    and name == manifest_name
                ):
                    raise InvalidBundle("manifest or frontend index is invalid")
            # tarfile stops on zero blocks before gzip may validate its CRC.
            if len(contents.fileobj.read(MAX_TRAILING_BYTES + 1)) > MAX_TRAILING_BYTES:
                raise InvalidBundle("release has excessive trailing content")
            if not (required | {manifest_name, frontend_name}).issubset(seen):
                raise InvalidBundle("full release lacks required binaries or assets")
            if installer is None:
                raise InvalidBundle("full release lacks an installer")

            # O_EXCL + NOFOLLOW in a private caller-owned directory; no archive
            # path is ever used as a destination. Keep partial outputs disposable.
            flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
            fd = os.open(output, flags, 0o500)
            try:
                stream = contents.extractfile(installer)
                if stream is None:
                    raise InvalidBundle("installer could not be read")
                with os.fdopen(fd, "wb") as destination, stream:
                    fd = -1
                    remaining = installer.size
                    while remaining:
                        chunk = stream.read(min(1024 * 1024, remaining))
                        if not chunk:
                            raise InvalidBundle("truncated installer")
                        destination.write(chunk)
                        remaining -= len(chunk)
                    destination.flush()
                    os.fsync(destination.fileno())
            finally:
                if fd != -1:
                    os.close(fd)
    except (tarfile.TarError, EOFError, OSError, UnicodeError) as error:
        raise InvalidBundle("invalid or truncated release archive") from error


def bootstrap(archive: Path, checksum_file: Path, output: Path) -> None:
    parent = output.parent.lstat()
    if not stat.S_ISDIR(parent.st_mode) or parent.st_uid != os.geteuid() or parent.st_mode & 0o077:
        raise InvalidBundle("installer destination must have a private owner-controlled parent")
    if output.exists() or output.is_symlink():
        raise InvalidBundle("installer destination already exists")
    try:
        _verify_archive(archive, _checksum(archive, checksum_file))
        _extract_verified(archive, output)
    except Exception:
        output.unlink(missing_ok=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--checksum", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        bootstrap(args.artifact, args.checksum, args.output)
    except (InvalidBundle, OSError) as error:
        print(f"verified installer bootstrap rejected artifact: {error}", file=sys.stderr)
        return 1
    print(f"verified installer extracted: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
