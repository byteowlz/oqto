#!/usr/bin/env python3
"""Validate dist/manifest.toml structure and source-path consistency."""

from __future__ import annotations

import argparse
import pathlib
import sys
import tomllib

ALLOWED_CLASSES = {"immutable_symlink", "mutable_copy_once", "runtime_generated"}
ALLOWED_INSTALL_METHODS = {
    "symlink_to_current",
    "copy",
    "copy_if_missing",
    "copy_template",
}
ALLOWED_PROFILES = {"personal", "team"}


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate dist/manifest.toml")
    parser.add_argument(
        "--manifest",
        default="dist/manifest.toml",
        help="Manifest path (default: dist/manifest.toml)",
    )
    parser.add_argument(
        "--allow-missing-binaries",
        action="store_true",
        help="Do not fail when binary source files under dist/immutable/bin are missing",
    )
    args = parser.parse_args()

    manifest_path = pathlib.Path(args.manifest)
    repo_root = pathlib.Path.cwd()
    errors: list[str] = []

    if not manifest_path.exists():
        print(f"error: manifest not found: {manifest_path}", file=sys.stderr)
        return 1

    try:
        data = tomllib.loads(manifest_path.read_text())
    except Exception as exc:
        print(f"error: failed to parse {manifest_path}: {exc}", file=sys.stderr)
        return 1

    if data.get("manifest_version") != 1:
        fail(errors, "manifest_version must be 1")

    release = data.get("release")
    if not isinstance(release, dict):
        fail(errors, "[release] section is required")
    else:
        for key in ("layout", "root", "current_symlink"):
            if key not in release:
                fail(errors, f"[release].{key} is required")

    assets = data.get("assets")
    if not isinstance(assets, list) or not assets:
        fail(errors, "[[assets]] must contain at least one entry")
        assets = []

    seen_ids: set[str] = set()

    for idx, asset in enumerate(assets):
        prefix = f"assets[{idx}]"
        if not isinstance(asset, dict):
            fail(errors, f"{prefix} must be a table")
            continue

        asset_id = asset.get("id")
        if not asset_id or not isinstance(asset_id, str):
            fail(errors, f"{prefix}.id is required")
        elif asset_id in seen_ids:
            fail(errors, f"duplicate asset id: {asset_id}")
        else:
            seen_ids.add(asset_id)

        cls = asset.get("class")
        if cls not in ALLOWED_CLASSES:
            fail(errors, f"{prefix}.class must be one of {sorted(ALLOWED_CLASSES)}")

        profiles = asset.get("profiles")
        if not isinstance(profiles, list) or not profiles:
            fail(errors, f"{prefix}.profiles must be a non-empty array")
        else:
            bad_profiles = [p for p in profiles if p not in ALLOWED_PROFILES]
            if bad_profiles:
                fail(errors, f"{prefix}.profiles has invalid entries: {bad_profiles}")

        install_path = asset.get("install_path")
        if not isinstance(install_path, str) or not install_path:
            fail(errors, f"{prefix}.install_path is required")

        if cls == "runtime_generated":
            if "source" in asset:
                fail(errors, f"{prefix} runtime_generated must not define source")
            if "install_method" in asset:
                fail(errors, f"{prefix} runtime_generated must not define install_method")
            continue

        source = asset.get("source")
        if not isinstance(source, str) or not source:
            fail(errors, f"{prefix}.source is required for {cls}")
        else:
            if source.startswith("/"):
                fail(errors, f"{prefix}.source must be repo-relative (got absolute path)")
            if not source.startswith("dist/"):
                fail(errors, f"{prefix}.source must live under dist/ (got {source})")

            source_path = repo_root / source
            missing_source = not source_path.exists()
            is_binary = str(asset.get("kind", "")) == "binary"
            if missing_source and not (is_binary and args.allow_missing_binaries):
                fail(errors, f"{prefix}.source does not exist: {source}")

        install_method = asset.get("install_method")
        if install_method not in ALLOWED_INSTALL_METHODS:
            fail(
                errors,
                f"{prefix}.install_method must be one of {sorted(ALLOWED_INSTALL_METHODS)}",
            )

    if errors:
        print("dist manifest validation failed:", file=sys.stderr)
        for err in errors:
            print(f"- {err}", file=sys.stderr)
        return 1

    print(f"dist manifest valid: {manifest_path} ({len(assets)} assets)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
