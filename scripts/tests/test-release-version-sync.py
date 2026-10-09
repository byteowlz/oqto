#!/usr/bin/env python3
"""Fail a release build if the Oqto dependency pin diverges from Cargo."""

from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

root = Path(__file__).resolve().parents[2]
cargo = tomllib.loads((root / "backend/Cargo.toml").read_text())
deps = tomllib.loads((root / "dependencies.toml").read_text())
workspace_version = cargo["workspace"]["package"]["version"]
pinned_version = deps["oqto"]["version"]
if workspace_version != pinned_version:
    raise SystemExit(
        f"release version mismatch: Cargo={workspace_version} dependencies.toml={pinned_version}"
    )
print(f"Oqto release version synchronized: {workspace_version}")
