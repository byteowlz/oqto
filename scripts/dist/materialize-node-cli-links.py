#!/usr/bin/env python3
"""Materialize contained Node CLI aliases for the no-symlink release format.

Execute the original JS target with adjacent pinned Node, preserving its module
resolution. Never dereference-copy a JS entry point into a different directory.
"""

import argparse
import os
import shlex
from pathlib import Path


def materialize(root: Path) -> list[dict[str, str]]:
    root = root.resolve(strict=True)
    node = root / "node/bin/node"
    if not node.is_file() or node.is_symlink():
        raise ValueError("runtime requires a regular adjacent Node executable")
    aliases = []
    for link in sorted(root.rglob("*")):
        if not link.is_symlink():
            continue
        relative = link.relative_to(root)
        if relative.parent != Path("node/bin") and link.parent.name != ".bin":
            raise ValueError(f"unsupported runtime symlink location: {relative}")
        target = link.resolve(strict=True)
        if not target.is_relative_to(root) or not target.is_file():
            raise ValueError(
                f"CLI alias must target a contained regular file: {relative}"
            )
        with target.open("rb") as source:
            header = source.read(128)
        if target.suffix not in {".js", ".mjs", ".cjs"} and not header.startswith(
            b"#!/usr/bin/env node"
        ):
            raise ValueError(
                f"CLI alias is not a qualified Node entrypoint: {relative}"
            )
        node_path = os.path.relpath(node, link.parent)
        target_path = os.path.relpath(target, link.parent)
        aliases.append(
            {
                "path": str(relative),
                "target": str(target.relative_to(root)),
                "node_path": node_path,
                "target_path": target_path,
            }
        )
    # Validate the entire alias set before changing any link.
    for alias in aliases:
        link = root / alias["path"]
        script = (
            '#!/bin/sh\nset -eu\nhere="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"\n'
        )
        script += f'exec "$here"/{shlex.quote(alias["node_path"])} "$here"/{shlex.quote(alias["target_path"])} "$@"\n'
        link.unlink()
        link.write_text(script)
        link.chmod(0o755)
    return [{"path": alias["path"], "target": alias["target"]} for alias in aliases]


if __name__ == "__main__":
    import json

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runtime", type=Path)
    args = parser.parse_args()
    print(json.dumps(materialize(args.runtime), indent=2))
