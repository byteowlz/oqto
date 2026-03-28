#!/usr/bin/env python3
"""Detect orphan Rust module files not reachable from crate roots.

Current scope: backend/crates/oqto/src (main crate only).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

SRC = Path("backend/crates/oqto/src")
MOD_RE = re.compile(r"^\s*(?:pub\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;")


def resolve_child_module(parent_file: Path, mod_name: str) -> Path | None:
    # In Rust:
    # - from mod.rs/main.rs/lib.rs, child modules live next to the file
    # - from foo.rs, child modules typically live under foo/
    sibling_dir = parent_file.parent
    nested_dir = parent_file.parent / parent_file.stem

    search_dirs: list[Path] = [sibling_dir]
    if parent_file.name not in {"mod.rs", "main.rs", "lib.rs"}:
        search_dirs = [nested_dir, sibling_dir]

    for base_dir in search_dirs:
        candidate1 = base_dir / f"{mod_name}.rs"
        candidate2 = base_dir / mod_name / "mod.rs"
        if candidate1.exists():
            return candidate1
        if candidate2.exists():
            return candidate2

    return None


def declared_modules(path: Path) -> list[str]:
    mods: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        m = MOD_RE.match(line)
        if m:
            mods.append(m.group(1))
    return mods


def reachable_module_files() -> set[Path]:
    roots = [p for p in [SRC / "main.rs", SRC / "lib.rs", SRC / "ctl/main.rs"] if p.exists()]
    roots.extend(sorted((SRC / "bin").glob("*.rs")))
    seen: set[Path] = set()
    queue: list[Path] = roots.copy()

    while queue:
        path = queue.pop(0)
        if path in seen:
            continue
        seen.add(path)

        for mod_name in declared_modules(path):
            child = resolve_child_module(path, mod_name)
            if child is None:
                # Module may be inline elsewhere or cfg-gated by platform; ignore here.
                continue
            if child not in seen:
                queue.append(child)

    return seen


def all_source_files() -> set[Path]:
    files: set[Path] = set()
    for p in SRC.rglob("*.rs"):
        rel = p.relative_to(SRC)
        if rel.parts and rel.parts[0] == "bin":
            continue
        files.add(p)
    return files


def main() -> int:
    reachable = reachable_module_files()
    all_files = all_source_files()
    orphans = sorted(p.relative_to(SRC).as_posix() for p in (all_files - reachable))

    if orphans:
        print("error: orphan module guardrail violations (unreachable .rs files):", file=sys.stderr)
        for rel in orphans:
            print(f"  - backend/crates/oqto/src/{rel}", file=sys.stderr)
        return 1

    print("Orphan module guardrail passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
