#!/usr/bin/env python3
"""Check the Git index, not ignored local state; see repository-layout.md."""

import subprocess
import sys
from pathlib import Path, PurePosixPath

ROOT_FILES = frozenset(
    {
        ".dockerignore",
        ".gitattributes",
        ".gitignore",
        "AGENTS.md",
        "CLAUDE.md",
        "CHANGELOG.md",
        "CONTEXT.md",
        "LICENSE",
        "README.md",
        "SETUP.md",
        "banner.png",
        "dependencies.toml",
        "design-system.toml",
        "justfile",
        "publish.toml",
        "rust-toolchain.toml",
        "setup.sh",
    }
)
ROOT_DIRECTORIES = frozenset(
    {
        ".ast-grep",
        ".githooks",
        ".github",
        ".oqto",
        ".pi",
        "artifacts",
        "backend",
        "contracts",
        "deploy",
        "design-exploration",
        "diagrams",
        "dist",
        "docs",
        "fixtures",
        "frontend",
        "logo",
        "machines",
        "mockups",
        "scripts",
        "skills",
        "tools",
    }
)
CACHE_DIRECTORIES = frozenset(
    {
        ".tmp",
        "__pycache__",
        "node_modules",
        ".pytest_cache",
        ".ruff_cache",
    }
)
GENERATED_PREFIXES = (
    "target/",
    "backend/target/",
    "frontend/dist/",
    "dist/out/",
    "dist/immutable/bin/",
    "dist/immutable/defaults/",
    "dist/immutable/frontend/",
    "dist/immutable/pi-runtime/",
    "dist/immutable/provider-login/",
)
SOURCE_MARKERS = frozenset({"dist/immutable/bin/.gitkeep"})


def violations(paths):
    """Return deterministic reasons for paths that do not belong in source."""
    findings = []
    for path in sorted(set(paths)):
        parts = PurePosixPath(path).parts
        if not parts:
            findings.append(f"{path!r}: empty source path")
            continue
        allowed = ROOT_FILES if len(parts) == 1 else ROOT_DIRECTORIES
        if parts[0] not in allowed:
            findings.append(
                f"{path!r}: unapproved root entry; update the layout contract deliberately"
            )
        if (
            CACHE_DIRECTORIES.intersection(parts[:-1])
            or path.endswith((".pyc", ".pyo"))
            or parts[-1] in {".DS_Store", "Thumbs.db"}
        ):
            findings.append(f"{path!r}: cache or scratch data must not be tracked")
        if path.startswith(GENERATED_PREFIXES) and path not in SOURCE_MARKERS:
            findings.append(
                f"{path!r}: generated staging belongs outside tracked source"
            )
    return findings


def check(root):
    """Read NUL-delimited index paths so unusual names cannot bypass policy."""
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "--cached", "-z"],
        capture_output=True,
        check=True,
    )
    paths = result.stdout.decode("utf-8", "surrogateescape").split("\0")
    return violations(path for path in paths if path)


def main():
    try:
        findings = check(Path(__file__).resolve().parents[2])
    except (OSError, subprocess.CalledProcessError) as error:
        print(
            f"Repository hygiene could not read the Git index: {error}", file=sys.stderr
        )
        return 1
    for finding in findings:
        print(finding, file=sys.stderr)
    if findings:
        return 1
    print("Repository hygiene: tracked layout and generated-file checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
