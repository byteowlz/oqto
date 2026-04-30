#!/usr/bin/env python3
"""Enforce high-level backend crate dependency boundaries.

This is intentionally small and dependency-free. It catches the architectural
mistake that hurts this refactor most: library/daemon crates growing upward
by depending on the `oqto` server crate instead of a focused domain/adapter
crate.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
CRATES_DIR = ROOT / "backend" / "crates"

# Existing transitional edge. Remove this allowlist entry as part of
# oqto-3ct7.3 / oqto-3ct7.11 once runner-client boundaries are extracted.
ALLOWED_OQTO_DEPENDENCIES = {
    "oqto-runner": "legacy runner daemon coupling; tracked by oqto-3ct7.3",
}

PACKAGE_RE = re.compile(r'^name\s*=\s*"([^"]+)"\s*$')
SECTION_RE = re.compile(r"^\s*\[([^]]+)]\s*$")
DEP_RE = re.compile(r'^\s*([A-Za-z0-9_.-]+)\s*=')
DEPENDENCY_SECTIONS = {
    "dependencies",
    "dev-dependencies",
    "build-dependencies",
    "target.'cfg(unix)'.dependencies",
    'target."cfg(unix)".dependencies',
}


def package_name(cargo_toml: Path) -> str:
    for line in cargo_toml.read_text().splitlines():
        match = PACKAGE_RE.match(line)
        if match:
            return match.group(1)
    raise ValueError(f"missing package name in {cargo_toml}")


def dependency_names(cargo_toml: Path) -> set[str]:
    deps: set[str] = set()
    section: str | None = None
    for line in cargo_toml.read_text().splitlines():
        section_match = SECTION_RE.match(line)
        if section_match:
            section = section_match.group(1)
            continue
        if section not in DEPENDENCY_SECTIONS:
            continue
        dep_match = DEP_RE.match(line)
        if dep_match:
            deps.add(dep_match.group(1))
    return deps


def main() -> int:
    violations: list[str] = []
    notes: list[str] = []

    for cargo_toml in sorted(CRATES_DIR.glob("*/Cargo.toml")):
        crate = package_name(cargo_toml)
        deps = dependency_names(cargo_toml)
        if crate == "oqto" or "oqto" not in deps:
            continue
        allowed_reason = ALLOWED_OQTO_DEPENDENCIES.get(crate)
        if allowed_reason:
            notes.append(f"allowed legacy edge: {crate} -> oqto ({allowed_reason})")
            continue
        violations.append(
            f"{cargo_toml}: {crate} depends on the oqto server crate. "
            "Move shared logic into a focused crate instead."
        )

    if notes:
        print("[backend-crate-boundaries] Transitional allowlist:")
        for note in notes:
            print(f"  - {note}")

    if violations:
        print("[backend-crate-boundaries] Forbidden crate dependency edges found:", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        return 1

    print("[backend-crate-boundaries] OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
