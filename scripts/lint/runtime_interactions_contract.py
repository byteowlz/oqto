"""Verify a complete generated runtime DTO bundle, not runtime or authority parity."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

from jsonschema import Draft7Validator

ROOT = Path(__file__).resolve().parents[2]
BUNDLE = Path("backend/crates/oqto-protocol/contracts/runtime-interactions/v1")
INPUTS = {
    "backend/crates/oqto-protocol/src/runtime_interactions.rs",
    "backend/crates/oqto-protocol/src/runtime_interactions/fixtures.json",
    "backend/crates/oqto-protocol/examples/export_runtime_interactions.rs",
    "backend/crates/oqto-protocol/Cargo.toml",
    "LICENSE",
}
# Earlier pins also recorded the workspace lockfile; they remain verifiable against
# their own exact commit. New bundles do not pin it: it churns on unrelated
# dependency changes and would invalidate consumer pins with no contract change.
INPUTS_WITH_LOCKFILE = INPUTS | {"backend/Cargo.lock"}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def source_bytes(root: Path, path: str, source_ref: str | None) -> bytes:
    if source_ref is None:
        return (root / path).read_bytes()
    return subprocess.run(
        ["git", "-C", str(root), "show", f"{source_ref}:{path}"],
        check=True,
        capture_output=True,
    ).stdout


def bundle_entries(bundle: Path, files: dict) -> dict[str, Path]:
    if bundle.is_symlink():
        raise ValueError("bundle must not be a symlink")
    members = list(bundle.rglob("*"))
    if any(
        path.is_symlink() or not (path.is_file() or path.is_dir()) for path in members
    ):
        raise ValueError("bundle must contain only regular files and directories")
    entries = {
        path.relative_to(bundle).as_posix(): path for path in members if path.is_file()
    }
    if set(files) | {"manifest.json"} != set(entries):
        raise ValueError("missing or extra bundle files")
    expected_dirs = {
        parent.as_posix()
        for name in files
        for parent in Path(name).parents
        if parent != Path(".")
    }
    actual_dirs = {
        path.relative_to(bundle).as_posix() for path in members if path.is_dir()
    }
    if expected_dirs != actual_dirs:
        raise ValueError("missing or extra bundle directories")
    return entries


def verify_files(bundle: Path, root: Path, source_ref: str | None) -> None:
    if source_ref is not None and not re.fullmatch(r"[a-f0-9]{40}", source_ref):
        raise ValueError("source ref must be a full immutable commit")
    if source_ref is not None:
        kind = subprocess.run(
            ["git", "-C", str(root), "cat-file", "-t", source_ref],
            check=True,
            capture_output=True,
        ).stdout.strip()
        if kind != b"commit":
            raise ValueError("source ref must identify a commit")
    if bundle.is_symlink() or (bundle / "manifest.json").is_symlink():
        raise ValueError("bundle metadata must be regular files, not symlinks")
    manifest = json.loads((bundle / "manifest.json").read_bytes())
    expected_inputs = INPUTS | {
        "backend/crates/oqto-protocol/src/runtime_interactions/source-fixtures.json"
    }
    # Historical immutable pins stay verifiable against their own exact commit,
    # including the workspace lockfile they used to record.
    if "backend/Cargo.lock" in manifest.get("inputs", {}):
        expected_inputs = INPUTS_WITH_LOCKFILE | {
            "backend/crates/oqto-protocol/src/runtime_interactions/source-fixtures.json"
        }
        if source_ref is not None and "source-fixtures.json" not in manifest.get(
            "files", {}
        ):
            expected_inputs = INPUTS_WITH_LOCKFILE
    elif source_ref is not None and "source-fixtures.json" not in manifest.get(
        "files", {}
    ):
        expected_inputs = INPUTS
    if (
        manifest.get("contract") != "oqto.runtime-interactions"
        or manifest.get("version") != 1
        or manifest.get("normative") is not True
        or set(manifest.get("inputs", {})) != expected_inputs
    ):
        raise ValueError("unexpected contract or source inputs")
    files = manifest["files"]
    entries = bundle_entries(bundle, files)
    for name, expected in files.items():
        if (
            Path(name).is_absolute()
            or ".." in Path(name).parts
            or sha256(entries[name].read_bytes()) != expected
        ):
            raise ValueError(f"file hash mismatch: {name}")
    for path, expected in manifest["inputs"].items():
        if sha256(source_bytes(root, path, source_ref)) != expected:
            raise ValueError(f"source hash mismatch: {path}")
    if source_ref is not None:
        for name, path in entries.items():
            if path.read_bytes() != source_bytes(
                root, (BUNDLE / name).as_posix(), source_ref
            ):
                raise ValueError(f"divergent pinned bundle: {name}")


def verify_operations(bundle: Path, fixtures: dict, request_schema: dict) -> None:
    operations = json.loads((bundle / "operations.json").read_bytes())
    expected = {
        case["value"]["operation"]["op"]: case["cli"] for case in fixtures["requests"]
    }
    variants = request_schema["definitions"]["RuntimeOperation"]["oneOf"]
    schema_operations = {variant["properties"]["op"]["enum"][0] for variant in variants}
    if set(expected) != set(operations) or set(expected) != schema_operations:
        raise ValueError("operation coverage mismatch")
    for op, cli in expected.items():
        row = operations[op]
        if row["cliPath"] != cli or row["implementation"] != "contract_only":
            raise ValueError(f"CLI contract mapping mismatch: {op}")
        if row["requiredAdapters"] != ["personal_tui", "managed_rpc", "cli_api"]:
            raise ValueError(f"missing adapter obligation: {op}")


def verify_corpus(bundle: Path) -> dict[str, int]:
    fixtures = json.loads((bundle / "fixtures.json").read_bytes())
    if fixtures.get("normative") is not True or fixtures.get("version") != 1:
        raise ValueError("non-normative or unsupported fixtures")
    validators = {}
    counts = {}
    for frame in ("request", "response", "event"):
        schema = json.loads((bundle / f"{frame}.schema.json").read_bytes())
        Draft7Validator.check_schema(schema)
        validators[frame] = Draft7Validator(schema)
        cases = fixtures[f"{frame}s"]
        for case in cases:
            validators[frame].validate(case["value"])
        counts[frame] = len(cases)
    for case in fixtures["invalid"]:
        if validators[case["frame"]].is_valid(case["value"]):
            raise ValueError(f"invalid fixture accepted: {case['name']}")
    counts["invalid"] = len(fixtures["invalid"])
    verify_operations(bundle, fixtures, validators["request"].schema)
    return counts


def verify_source_corpus(bundle: Path) -> dict[str, int]:
    path = bundle / "source-fixtures.json"
    if not path.exists():
        return {}
    fixtures = json.loads(path.read_bytes())
    if fixtures.get("normative") is not True or fixtures.get("version") != 1:
        raise ValueError("non-normative source corpus")
    validators = {}
    for name in ("request", "response", "frame"):
        schema = json.loads((bundle / f"source-{name}.schema.json").read_bytes())
        Draft7Validator.check_schema(schema)
        validators[name] = Draft7Validator(schema)
    for case in fixtures["pairs"] + fixtures["mismatches"]:
        validators["request"].validate(case["request"])
        validators["response"].validate(case["response"])
    for case in fixtures["frames"]:
        validators["frame"].validate(case["value"])
    for case in fixtures["invalid"]:
        if validators["frame"].is_valid(case["value"]):
            raise ValueError(f"invalid source fixture accepted: {case['name']}")
    operations = json.loads((bundle / "operations.json").read_bytes())
    if {case["request"]["operation"]["op"] for case in fixtures["pairs"]} != set(
        operations
    ):
        raise ValueError("source operation coverage mismatch")
    return {
        f"source_{name}": len(fixtures[name])
        for name in ("pairs", "frames", "invalid", "mismatches")
    }


def verify(
    bundle: Path, root: Path = ROOT, source_ref: str | None = None
) -> dict[str, int]:
    verify_files(bundle, root, source_ref)
    return verify_corpus(bundle) | verify_source_corpus(bundle)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, default=ROOT / BUNDLE)
    parser.add_argument(
        "--source-ref", help="full Core commit for vendored bundle comparison"
    )
    args = parser.parse_args()
    counts = verify(args.bundle, source_ref=args.source_ref)
    print(
        f"Runtime DTO/codec bundle verified: {counts}; runtime/CLI execution not claimed"
    )


if __name__ == "__main__":
    main()
