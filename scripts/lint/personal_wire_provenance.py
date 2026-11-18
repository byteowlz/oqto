"""Read-only whole-crate provenance gate; not a runtime compatibility certificate.

Use a full immutable Core commit and the complete vendored crate. Cargo workspace
inheritance may become explicit; optional authors and prose are nonsemantic.
Dependencies, features and all other manifest values must otherwise match exactly.
The upstream root LICENSE is included when the crate has no separate license file.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import subprocess
import tomllib
from pathlib import Path

CRATE = "backend/crates/oqto-personal-wire"
FULL_REVISION = re.compile(r"[0-9a-f]{40}\Z")
MAX_FILE_BYTES = 2 * 1024 * 1024


def read_vendor(root: Path, relative: str) -> bytes:
    """Never follow a source-file or source-directory symlink."""
    candidate = root / relative
    if not candidate.resolve().is_relative_to(root.resolve()):
        raise ValueError("file escapes vendor root")
    for part in (candidate, *candidate.parents):
        if part == root:
            break
        if part.is_symlink():
            raise ValueError("symlinked vendor content")
    if not candidate.is_file() or candidate.stat().st_size > MAX_FILE_BYTES:
        raise ValueError("missing, nonregular or oversized file")
    return candidate.read_bytes()


def inherited_dependency(spec: dict, base: str | dict) -> dict:
    if set(spec) - {"workspace", "optional", "features"}:
        raise ValueError("unsupported dependency inheritance")
    inherited = {"version": base} if isinstance(base, str) else copy.deepcopy(base)
    if "optional" in spec:
        inherited["optional"] = spec["optional"]
    if "features" in spec:
        inherited["features"] = sorted(
            set(inherited.get("features", [])) | set(spec["features"])
        )
    return inherited


def portable_manifest(manifest: dict, workspace: dict) -> dict:
    """Resolve only supported Cargo inheritance, preserving security-relevant data."""
    result = copy.deepcopy(manifest)
    package = result["package"]
    for key, value in package.items():
        if isinstance(value, dict) and value.get("workspace") is True:
            if set(value) != {"workspace"}:
                raise ValueError("unsupported package inheritance")
            package[key] = workspace["package"][key]
    for name, spec in result.get("dependencies", {}).items():
        if not isinstance(spec, dict) or spec.get("workspace") is not True:
            continue
        result["dependencies"][name] = inherited_dependency(
            spec, workspace["dependencies"][name]
        )
    return result


def check_cargo(actual: bytes, canonical: bytes, workspace: bytes) -> None:
    """Allow packaging prose/author omission, not dependency or feature drift."""
    expected = portable_manifest(
        tomllib.loads(canonical.decode()),
        tomllib.loads(workspace.decode())["workspace"],
    )
    received = tomllib.loads(actual.decode())
    expected_package, received_package = expected["package"], received["package"]
    if "authors" in received_package and received_package[
        "authors"
    ] != expected_package.get("authors"):
        raise ValueError("Cargo authors differ")
    for package in (expected_package, received_package):
        package.pop("description", None)
        package.pop("authors", None)
    # Cargo treats string and version-only table dependencies equivalently.
    for manifest in (expected, received):
        for name, spec in manifest.get("dependencies", {}).items():
            if isinstance(spec, str):
                manifest["dependencies"][name] = {"version": spec}
    if received != expected:
        raise ValueError("Cargo normalization differs from canonical manifest")


def vendor_files(root: Path) -> set[str]:
    files = set()
    for directory, subdirs, names in os.walk(root, followlinks=False):
        parent = Path(directory)
        files.update((parent / name).relative_to(root).as_posix() for name in names)
        files.update(
            (parent / name).relative_to(root).as_posix()
            for name in subdirs
            if (parent / name).is_symlink()
        )
    return files


def read_metadata(
    vendor: Path, revision: str, paths: set[str]
) -> tuple[dict, list[str]]:
    metadata = json.loads(read_vendor(vendor, "SOURCE.json"))
    problems = []
    if vendor_files(vendor) != paths | {"SOURCE.json", "PROVENANCE.md"}:
        problems.append("vendor: file set differs from canonical crate plus provenance")
    if metadata.get("source") != revision:
        problems.append("SOURCE.json: source revision mismatch")
    hashes = metadata.get("files")
    if not isinstance(hashes, dict):
        raise TypeError("SOURCE.json files must be an object")
    if set(hashes) != paths:
        problems.append("SOURCE.json: file set differs from complete canonical crate")
    if revision not in read_vendor(vendor, "PROVENANCE.md").decode():
        problems.append("PROVENANCE.md: full selected revision absent")
    return hashes, problems


def verify_vendor(
    vendor: Path, revision: str, canonical: dict[str, bytes], workspace: bytes
) -> list[str]:
    """Verify machine hashes, source bytes, complete coverage and normalized Cargo."""
    if not FULL_REVISION.fullmatch(revision):
        return ["source revision must be a full lowercase 40-character commit SHA"]
    if "Cargo.toml" not in canonical:
        return ["canonical source is missing Cargo.toml"]
    try:
        hashes, problems = read_metadata(vendor, revision, set(canonical))
    except (OSError, ValueError, TypeError, AttributeError):
        return ["SOURCE.json/PROVENANCE.md: missing, unsafe or invalid metadata"]
    for relative, source in canonical.items():
        try:
            actual = read_vendor(vendor, relative)
            if hashes.get(relative) != hashlib.sha256(actual).hexdigest():
                problems.append(
                    f"{relative}: recorded SHA256 differs from vendored bytes"
                )
            if relative == "Cargo.toml":
                check_cargo(actual, source, workspace)
            elif actual != source:
                problems.append(
                    f"{relative}: bytes differ from selected canonical revision"
                )
        except (OSError, ValueError, KeyError, TypeError, AttributeError):
            problems.append(
                f"{relative}: unsafe/missing content or unsupported normalization"
            )
    return problems


def load_canonical(repo: Path, revision: str) -> tuple[dict[str, bytes], bytes]:
    git = ["git", "-C", str(repo)]
    tree = subprocess.run(
        [*git, "ls-tree", "-r", "--name-only", "-z", f"{revision}:{CRATE}"],
        check=True,
        capture_output=True,
    ).stdout
    locations = {
        raw.decode(): f"{CRATE}/{raw.decode()}" for raw in tree.split(b"\0") if raw
    }
    locations.setdefault("LICENSE", "LICENSE")
    locations["@workspace"] = "backend/Cargo.toml"
    files = {
        relative: subprocess.run(
            [*git, "show", f"{revision}:{source}"], check=True, capture_output=True
        ).stdout
        for relative, source in locations.items()
    }
    return files, files.pop("@workspace")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vendor", type=Path, required=True)
    parser.add_argument("--source-ref", required=True)
    parser.add_argument(
        "--repo", type=Path, default=Path(__file__).resolve().parents[2]
    )
    args = parser.parse_args()
    if not FULL_REVISION.fullmatch(args.source_ref):
        parser.error("--source-ref must be a full lowercase 40-character commit SHA")
    try:
        canonical, workspace = load_canonical(args.repo, args.source_ref)
        problems = verify_vendor(
            args.vendor.resolve(), args.source_ref, canonical, workspace
        )
    except (OSError, ValueError, subprocess.CalledProcessError):
        print(
            "FAIL: selected canonical source is unavailable; no fetch or mutation attempted"
        )
        return 1
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}")
        return 1
    print(
        "PASS: complete crate provenance; runtime/handler/consumer gates remain separate"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
