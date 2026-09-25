#!/usr/bin/env python3
"""Fail a release before packaging if tag, Cargo and dependency versions diverge."""

import argparse
from pathlib import Path
import re

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


def check(tag: str, cargo_path: Path, deps_path: Path) -> None:
    if not re.fullmatch(r"v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?", tag):
        raise ValueError("release requires an explicit v<semver> tag")
    cargo = tomllib.loads(cargo_path.read_text())
    deps = tomllib.loads(deps_path.read_text())
    declared = cargo["workspace"]["package"]["version"]
    pinned = deps["oqto"]["version"]
    if declared != tag[1:] or pinned != tag[1:]:
        raise ValueError(
            f"release tag {tag} does not match Cargo ({declared}) and dependency pin ({pinned})"
        )


def main() -> None:
    root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True, help="Explicit release tag, e.g. v0.5.0")
    parser.add_argument("--cargo", type=Path, default=root / "backend/Cargo.toml")
    parser.add_argument("--dependencies", type=Path, default=root / "dependencies.toml")
    args = parser.parse_args()
    try:
        check(args.tag, args.cargo, args.dependencies)
    except (ValueError, KeyError, OSError, tomllib.TOMLDecodeError) as error:
        parser.exit(1, f"release preflight failed: {error}\n")
    print(f"release tag and declared versions agree: {args.tag}")


if __name__ == "__main__":
    main()
