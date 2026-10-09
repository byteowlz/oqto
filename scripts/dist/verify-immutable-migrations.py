#!/usr/bin/env python3
"""Fail when a migration already declared immutable changes bytes."""

from __future__ import annotations

import hashlib
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
EXPECTED = {
    "backend/crates/oqto/migrations/20260901001_oqto_apps_v0.sql":
        "fcfbad8cb2ba5bbed2b561b4f5b7e72b932c1639a2f2ad261faf7ed16624e59399af5dba723f177c4fa26995ec1e84fb",
}

errors: list[str] = []
for relative, expected in EXPECTED.items():
    path = ROOT / relative
    actual = hashlib.sha384(path.read_bytes()).hexdigest() if path.is_file() else "missing"
    if actual != expected:
        errors.append(f"{relative}: expected sha384 {expected}, got {actual}")

if errors:
    print("immutable migration verification failed:", file=sys.stderr)
    for error in errors:
        print(f"  {error}", file=sys.stderr)
    print("Add a new migration; never edit an applied migration.", file=sys.stderr)
    raise SystemExit(1)

print(f"immutable migrations verified: {len(EXPECTED)}")
