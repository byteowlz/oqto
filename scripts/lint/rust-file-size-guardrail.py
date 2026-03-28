#!/usr/bin/env python3
"""Rust file-size ratchet guardrail.

Fails when a Rust source file exceeds its allowed line budget.
Budgets come from a baseline JSON map (path -> max lines).

New files default to DEFAULT_NEW_FILE_MAX lines unless added to baseline.
Use --update to refresh baseline with current sizes.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

DEFAULT_NEW_FILE_MAX = 800
BASELINE_PATH = Path("scripts/lint/rust-file-size-baseline.json")
SCAN_ROOT = Path("backend/crates")


def gather_rust_files() -> list[Path]:
    files: list[Path] = []
    for p in SCAN_ROOT.rglob("*.rs"):
        # Skip build output and vendored deps if present
        if any(part in {"target", ".git"} for part in p.parts):
            continue
        files.append(p)
    return sorted(files)


def count_lines(path: Path) -> int:
    with path.open("r", encoding="utf-8") as f:
        return sum(1 for _ in f)


def load_baseline(path: Path) -> dict[str, int]:
    if not path.exists():
        return {}
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError(f"baseline file must be a JSON object: {path}")
    out: dict[str, int] = {}
    for k, v in data.items():
        if not isinstance(k, str) or not isinstance(v, int):
            raise ValueError(f"invalid baseline entry: {k} -> {v}")
        out[k] = v
    return out


def save_baseline(path: Path, baseline: dict[str, int]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    ordered = dict(sorted(baseline.items()))
    path.write_text(json.dumps(ordered, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--update", action="store_true", help="rewrite baseline from current sizes")
    parser.add_argument("--baseline", default=str(BASELINE_PATH))
    parser.add_argument("--default-max", type=int, default=DEFAULT_NEW_FILE_MAX)
    args = parser.parse_args()

    baseline_path = Path(args.baseline)
    baseline = load_baseline(baseline_path)

    files = gather_rust_files()
    current: dict[str, int] = {}
    violations: list[str] = []
    new_files: list[str] = []

    for path in files:
        rel = path.as_posix()
        lines = count_lines(path)
        current[rel] = lines

        allowed = baseline.get(rel, args.default_max)
        if rel not in baseline:
            new_files.append(rel)

        if lines > allowed:
            source = "baseline" if rel in baseline else f"default max ({args.default_max})"
            violations.append(f"{rel}: {lines} lines > {allowed} ({source})")

    if args.update:
        # Ratchet down automatically: store current size as new cap for every tracked file.
        save_baseline(baseline_path, current)
        print(f"Updated {baseline_path} with {len(current)} Rust files.")
        return 0

    if not baseline:
        print(
            f"error: baseline missing: {baseline_path}. Run: python3 scripts/lint/rust-file-size-guardrail.py --update",
            file=sys.stderr,
        )
        return 2

    if new_files:
        print(
            f"error: detected {len(new_files)} Rust files not present in baseline. "
            "Run: python3 scripts/lint/rust-file-size-guardrail.py --update",
            file=sys.stderr,
        )
        for rel in new_files[:20]:
            print(f"  - {rel}", file=sys.stderr)
        if len(new_files) > 20:
            print(f"  ... and {len(new_files) - 20} more", file=sys.stderr)
        return 1

    if violations:
        print("error: rust file-size guardrail violations:", file=sys.stderr)
        for v in violations:
            print(f"  - {v}", file=sys.stderr)
        return 1

    print(f"Rust file-size guardrail passed for {len(files)} files.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
