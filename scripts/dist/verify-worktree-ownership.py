#!/usr/bin/env python3
"""Fail before dist tooling mutates a repository owned by another user."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import sys


def ownership_errors(root: Path, effective_uid: int) -> list[str]:
    root_uid = root.stat().st_uid
    errors: list[str] = []
    if effective_uid == 0 and root_uid != 0:
        errors.append(
            "dist tooling must not run as root in a user-owned checkout; "
            "run `just deploy` as the checkout owner and let it request sudo only for installation"
        )
        return errors

    for relative in ("dist", "frontend/dist"):
        target = root / relative
        if not target.exists():
            continue
        for path in (target, *target.rglob("*")):
            try:
                owner = path.lstat().st_uid
            except FileNotFoundError:
                continue
            if owner != effective_uid:
                errors.append(
                    f"{path.relative_to(root)} is owned by uid {owner}, expected uid {effective_uid}"
                )
                if len(errors) == 8:
                    errors.append("additional ownership mismatches omitted")
                    return errors
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="repository root (defaults to this script's repository)",
    )
    args = parser.parse_args()
    root = args.root.resolve()
    errors = ownership_errors(root, os.geteuid())
    if not errors:
        return 0

    print("[dist-ownership] refusing to mutate build outputs:", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)
    print(
        f"Repair existing outputs with: sudo chown -R $(id -un):$(id -gn) "
        f"{root / 'dist'} {root / 'frontend/dist'}",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
