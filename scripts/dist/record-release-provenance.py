#!/usr/bin/env python3
"""Record release identity in a staged manifest, or refuse an ambiguous build.

The staged artifact must say exactly which declared version and which commit it
was built from. The requested release version must equal the Cargo workspace
version; a worktree with uncommitted changes is refused unless the caller
explicitly marks the build dirty (then `oqto-setup install` refuses it again
unless the operator passes --allow-dirty-source).
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path

SEMVER = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


class ProvenanceError(Exception):
    pass


def workspace_version(root: Path) -> str:
    cargo = tomllib.loads((root / "backend/Cargo.toml").read_text())
    try:
        return cargo["workspace"]["package"]["version"]
    except KeyError as missing:
        raise ProvenanceError(
            "backend/Cargo.toml lacks [workspace.package] version"
        ) from missing


def git(root: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(root), *args], check=True, capture_output=True, text=True
    ).stdout


def source_state(root: Path) -> tuple[str, bool]:
    commit = git(root, "rev-parse", "--verify", "HEAD").strip()
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ProvenanceError(f"unexpected HEAD commit id: {commit!r}")
    dirty = bool(git(root, "status", "--porcelain", "--untracked-files=normal").strip())
    return commit, dirty


DEV_VERSION = re.compile(
    r"^(?P<core>.+)-dev\.(?P<stamp>\d{14})\.g(?P<sha>[0-9a-f]{8})(?P<dirty>\.dirty)?$"
)


def dev_version(version: str, declared: str, commit: str, dirty: bool) -> bool:
    """Validate a dev build version; False when it is not a dev version.

    A dev version must describe its own source truthfully: declared core,
    the actual commit, and `.dirty` exactly when the tree is dirty.
    """
    match = DEV_VERSION.fullmatch(version)
    if not match:
        return False
    if match["core"] != declared:
        raise ProvenanceError(
            f"dev version core {match['core']} does not match the Cargo workspace version {declared}"
        )
    if not commit.startswith(match["sha"]):
        raise ProvenanceError(
            f"dev version names commit g{match['sha']} but HEAD is {commit[:8]}"
        )
    if bool(match["dirty"]) != dirty:
        state = "dirty" if dirty else "clean"
        raise ProvenanceError(
            f"dev version {'claims' if match['dirty'] else 'omits'} .dirty but the worktree is {state}"
        )
    return True


def requested_version(requested: str, declared: str) -> str:
    version = requested[1:] if requested.startswith("v") else requested
    if not SEMVER.fullmatch(version):
        raise ProvenanceError(f"release version {requested!r} is not semantic")
    if version != declared:
        raise ProvenanceError(
            f"release version {version} does not match the Cargo workspace version {declared}; "
            "use `just bump` instead of packaging an undeclared version"
        )
    return version


def stamp(manifest: Path, version: str, commit: str, dirty: bool) -> None:
    lines = manifest.read_text().splitlines()
    data = tomllib.loads("\n".join(lines))
    release = data.get("release")
    if not isinstance(release, dict):
        raise ProvenanceError("staged manifest has no [release] table")
    present = {"version", "source_commit", "source_dirty"} & release.keys()
    if present:
        raise ProvenanceError(f"staged manifest already declares {sorted(present)}")
    index = lines.index("[release]") + 1
    lines[index:index] = [
        f'version = "{version}"',
        f'source_commit = "{commit}"',
        f"source_dirty = {'true' if dirty else 'false'}",
    ]
    manifest.write_text("\n".join(lines) + "\n")
    stamped = tomllib.loads(manifest.read_text())["release"]
    if (stamped["version"], stamped["source_commit"], stamped["source_dirty"]) != (
        version,
        commit,
        dirty,
    ):
        raise ProvenanceError("failed to record release provenance")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--allow-dirty", action="store_true")
    args = parser.parse_args()
    try:
        declared = workspace_version(args.root)
        commit, dirty = source_state(args.root)
        # Dev builds (`just deploy --dev`) may come from a dirty tree; their
        # version records that, and oqto-setup still needs explicit consent.
        is_dev = dev_version(args.version, declared, commit, dirty)
        version = args.version if is_dev else requested_version(args.version, declared)
        if dirty and not (is_dev or args.allow_dirty):
            raise ProvenanceError(
                "refusing to package from a worktree with uncommitted or untracked changes; "
                "commit or stash them (OQTO_DIST_ALLOW_DIRTY=1 records a dirty build that "
                "oqto-setup install will refuse without --allow-dirty-source)"
            )
        stamp(args.manifest, version, commit, dirty)
    except (
        ProvenanceError,
        subprocess.CalledProcessError,
        OSError,
        tomllib.TOMLDecodeError,
    ) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"release {version} from {commit}{' (dirty)' if dirty else ''}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
