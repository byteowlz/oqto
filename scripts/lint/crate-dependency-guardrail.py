#!/usr/bin/env python3
"""Enforce workspace crate dependency direction constraints."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

BACKEND_DIR = Path("backend")
BASELINE_PATH = Path("scripts/lint/crate-dependency-guardrail-baseline.json")

# source crate -> forbidden target crates
FORBIDDEN: dict[str, set[str]] = {
    "oqto-runner": {"oqto"},
    "oqto-protocol": {"oqto", "oqto-runner"},
    "oqto-runner-protocol": {"oqto", "oqto-runner"},
    "oqto-files": {"oqto"},
}

# source crate -> required target crates
REQUIRED: dict[str, set[str]] = {
    "oqto": {"oqto-runner-protocol"},
    "oqto-runner": {"oqto-runner-protocol"},
}

# Files that must remain thin re-exports to prevent protocol drift.
REEXPORT_REQUIREMENTS: dict[Path, str] = {
    Path("backend/crates/oqto/src/runner/protocol.rs"): "pub use oqto_runner_protocol::*;",
    Path("backend/crates/oqto-runner/src/protocol.rs"): "pub use oqto_runner_protocol::*;",
}


def load_baseline() -> set[str]:
    if not BASELINE_PATH.exists():
        return set()
    data = json.loads(BASELINE_PATH.read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise ValueError("crate dependency baseline must be a JSON array")
    out: set[str] = set()
    for item in data:
        if not isinstance(item, str):
            raise ValueError("crate dependency baseline entries must be strings")
        out.add(item)
    return out


def main() -> int:
    update = "--update" in sys.argv

    cmd = ["cargo", "metadata", "--format-version", "1", "--no-deps"]
    proc = subprocess.run(
        cmd,
        cwd=BACKEND_DIR,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        print("error: failed to run cargo metadata", file=sys.stderr)
        print(proc.stderr, file=sys.stderr)
        return 2

    data = json.loads(proc.stdout)
    packages = data.get("packages", [])
    deps: dict[str, set[str]] = {}
    for pkg in packages:
        name = pkg["name"]
        deps[name] = {d["name"] for d in pkg.get("dependencies", [])}

    violations: set[str] = set()
    for src, forbidden_targets in FORBIDDEN.items():
        src_deps = deps.get(src, set())
        for target in sorted(forbidden_targets):
            if target in src_deps:
                violations.add(f"forbidden dependency: {src} -> {target}")

    for src, required_targets in REQUIRED.items():
        src_deps = deps.get(src, set())
        for target in sorted(required_targets):
            if target not in src_deps:
                violations.add(f"missing dependency: {src} -> {target}")

    for path, expected in REEXPORT_REQUIREMENTS.items():
        if not path.exists():
            violations.add(f"missing required re-export file: {path}")
            continue
        actual = path.read_text(encoding="utf-8").strip()
        if actual != expected:
            violations.add(
                f"protocol re-export drift: {path} must contain exactly '{expected}'"
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
        print("error: crate dependency guardrail violations:", file=sys.stderr)
        for v in new_violations:
            print(f"  - {v}", file=sys.stderr)
        print(
            f"hint: existing debt is tracked in {BASELINE_PATH}; remove baseline entries as violations are fixed",
            file=sys.stderr,
        )
        return 1

    print("Crate dependency guardrail passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
