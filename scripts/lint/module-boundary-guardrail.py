#!/usr/bin/env python3
"""Guardrail for import boundaries inside backend/crates/oqto/src."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path("backend/crates/oqto/src")
BASELINE_PATH = Path("scripts/lint/module-boundary-guardrail-baseline.json")
USE_RE = re.compile(r"^\s*use\s+crate::([a-zA-Z0-9_:]+)")

RULES = [
    {
        "from_prefix": "api/",
        "forbid": ["runner::daemon"],
        "message": "api layer must not import runner daemon internals; use runner client/protocol/router",
    },
    {
        "from_prefix": "runner/",
        "forbid": ["api::"],
        "message": "runner layer must not import api layer",
    },
    {
        "from_prefix": "session/",
        "forbid": ["api::ws_multiplexed"],
        "message": "session layer must not import websocket channel implementation",
    },
]


def iter_rs_files() -> list[Path]:
    files: list[Path] = []
    for p in ROOT.rglob("*.rs"):
        if "target" in p.parts:
            continue
        files.append(p)
    return sorted(files)


def load_baseline() -> set[str]:
    if not BASELINE_PATH.exists():
        return set()
    data = json.loads(BASELINE_PATH.read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise ValueError("module boundary baseline must be a JSON array")
    out: set[str] = set()
    for item in data:
        if not isinstance(item, str):
            raise ValueError("module boundary baseline entries must be strings")
        out.add(item)
    return out


def main() -> int:
    update = "--update" in sys.argv
    violations: set[str] = set()

    for path in iter_rs_files():
        rel = path.relative_to(ROOT).as_posix()
        text = path.read_text(encoding="utf-8")
        for idx, line in enumerate(text.splitlines(), start=1):
            m = USE_RE.match(line)
            if not m:
                continue
            import_path = m.group(1)
            for rule in RULES:
                if not rel.startswith(rule["from_prefix"]):
                    continue
                for forbidden in rule["forbid"]:
                    if import_path.startswith(forbidden):
                        violations.add(
                            f"{rel}:{idx}: use crate::{import_path} violates rule: {rule['message']}"
                        )

    if update:
        BASELINE_PATH.parent.mkdir(parents=True, exist_ok=True)
        BASELINE_PATH.write_text(
            json.dumps(sorted(violations), indent=2) + "\n", encoding="utf-8"
        )
        print(f"Updated {BASELINE_PATH} with {len(violations)} baseline violations.")
        return 0

    baseline = load_baseline()
    new_violations = sorted(v for v in violations if v not in baseline)

    if new_violations:
        print("error: module boundary guardrail violations:", file=sys.stderr)
        for v in new_violations:
            print(f"  - {v}", file=sys.stderr)
        print(
            f"hint: existing debt is tracked in {BASELINE_PATH}; remove entries as violations are fixed",
            file=sys.stderr,
        )
        return 1

    print("Module boundary guardrail passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
