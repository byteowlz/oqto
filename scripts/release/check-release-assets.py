#!/usr/bin/env python3
"""Fail closed before publishing a non-canonical Linux full release bundle.

This checks structure and declared target, not publisher authenticity or runtime
health. The release job must still acquire independent signing and native proof.
"""

import argparse
from pathlib import Path, PurePosixPath
import tarfile

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

TARGET = "x86_64-unknown-linux-gnu"
REQUIRED_BINS = (
    "oqto", "oqtoctl", "oqto-setup", "oqto-runner", "oqto-files",
    "oqto-sandbox", "oqto-usermgr", "pi-bridge",
)


def check_artifact(artifact: Path, tag: str) -> None:
    root = f"oqto-{tag}-{TARGET}"
    if artifact.name != f"{root}.tar.gz":
        raise ValueError(f"unexpected release asset name: {artifact.name}")
    required = {f"{root}/manifest.toml", f"{root}/immutable/frontend/dist/index.html"}
    required.update(f"{root}/immutable/bin/{name}" for name in REQUIRED_BINS)
    with tarfile.open(artifact, "r:gz") as archive:
        entries = {}
        for member in archive.getmembers():
            name = member.name.removeprefix("./").rstrip("/")
            parts = PurePosixPath(name).parts
            if not parts or parts[0] != root or ".." in parts or member.issym() or member.islnk():
                raise ValueError(f"unsafe release member: {member.name}")
            if not member.isfile() and not member.isdir():
                raise ValueError(f"non-file release member: {member.name}")
            if name in entries:
                raise ValueError(f"duplicate release member: {member.name}")
            entries[name] = member
        missing = required - entries.keys()
        if missing:
            raise ValueError(f"incomplete full release; missing {sorted(missing)}")
        for name in required:
            if not entries[name].isfile():
                raise ValueError(f"required release member is not a regular file: {name}")
        for bin_name in REQUIRED_BINS:
            member = entries[f"{root}/immutable/bin/{bin_name}"]
            if member.mode & 0o111 == 0:
                raise ValueError(f"release binary is not executable: {bin_name}")
        manifest_member = entries[f"{root}/manifest.toml"]
        if manifest_member.size > 1_048_576:
            raise ValueError("release manifest exceeds 1 MiB")
        stream = archive.extractfile(manifest_member)
        if stream is None:
            raise ValueError("release manifest is unreadable")
        manifest = tomllib.loads(stream.read().decode("utf-8"))
        if (manifest.get("manifest_version"), manifest.get("id"),
                manifest.get("release", {}).get("target")) != (1, "oqto-dist", "full"):
            raise ValueError("release manifest does not declare the full target")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    try:
        matches = list(args.directory.rglob(f"oqto-{args.tag}-{TARGET}.tar.gz"))
        if len(matches) != 1:
            raise ValueError(f"expected exactly one canonical Linux bundle; found {len(matches)}")
        check_artifact(matches[0], args.tag)
    except (ValueError, OSError, tarfile.TarError, UnicodeError, tomllib.TOMLDecodeError) as error:
        parser.exit(1, f"release asset preflight failed: {error}\n")
    print(f"canonical full release asset validated: {matches[0]}")


if __name__ == "__main__":
    main()
