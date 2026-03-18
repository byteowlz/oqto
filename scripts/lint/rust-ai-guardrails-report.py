#!/usr/bin/env python3
"""Report ast-grep Rust AI guardrail violations (totals, by rule, by file).

Usage:
  scripts/lint/rust-ai-guardrails-report.py [--paths backend/crates ...] [--top 20]
  scripts/lint/rust-ai-guardrails-report.py --changed
"""

from __future__ import annotations

import argparse
import collections
import json
import pathlib
import re
import subprocess
import sys
from typing import Iterable, List

ROOT = pathlib.Path(__file__).resolve().parents[2]
CFG = ROOT / ".ast-grep" / "sgconfig.yml"


def _run(cmd: List[str]) -> str:
    return subprocess.check_output(cmd, text=True, cwd=ROOT).strip()


def _get_changed_rs() -> List[str]:
    try:
        _run(["git", "rev-parse", "--verify", "origin/main"])
        base = _run(["git", "merge-base", "origin/main", "HEAD"])
    except Exception:
        base = "HEAD~1"

    out = _run(
        [
            "git",
            "diff",
            "--name-only",
            "--diff-filter=ACMR",
            f"{base}...HEAD",
            "--",
            "*.rs",
        ]
    )
    paths = [p for p in out.splitlines() if p]
    pat = re.compile(r"(^|/)(tests?|testdata|fixtures)/|_test\.rs$|/target/")
    return [p for p in paths if not pat.search(p)]


def _scan(paths: Iterable[str]):
    cmd = [
        "ast-grep",
        "scan",
        "--config",
        str(CFG),
        "--json=stream",
        *paths,
    ]
    proc = subprocess.Popen(
        cmd,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    by_rule = collections.Counter()
    by_file = collections.Counter()
    total = 0

    assert proc.stdout is not None
    for line in proc.stdout:
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        total += 1
        by_rule[obj.get("ruleId", "<unknown>")] += 1
        by_file[obj.get("file", "<unknown>")] += 1

    stderr = ""
    if proc.stderr:
        stderr = proc.stderr.read().strip()
    code = proc.wait()
    if code != 0:
        print(f"ast-grep scan failed (exit={code})", file=sys.stderr)
        if stderr:
            print(stderr, file=sys.stderr)
        sys.exit(code)

    return total, by_rule, by_file


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--paths", nargs="*", default=["backend/crates"], help="paths to scan")
    p.add_argument("--changed", action="store_true", help="scan only changed Rust files vs origin/main")
    p.add_argument("--top", type=int, default=20, help="top files to print")
    args = p.parse_args()

    paths = args.paths
    if args.changed:
        changed = _get_changed_rs()
        if not changed:
            print("No changed Rust files matched.")
            return 0
        paths = changed

    total, by_rule, by_file = _scan(paths)

    print(f"TOTAL\t{total}")
    print("BY_RULE")
    for rule, count in by_rule.most_common():
        print(f"{rule}\t{count}")
    print(f"FILES_WITH_HITS\t{len(by_file)}")
    print("TOP_FILES")
    for file, count in by_file.most_common(args.top):
        print(f"{count}\t{file}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
