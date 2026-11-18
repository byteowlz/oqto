"""Verify a vendored runtime-interaction bundle from any repository.

Two questions, both fail-closed:

1. Are the vendored bytes the pinned Core bundle? `SOURCE.json` must name the
   Core commit and manifest hash that produced these exact files; the manifest
   hash is compared to the bytes on disk and every file is compared to the
   pinned Core commit, so an edited file, a regenerated manifest or a rehashed
   copy is refused.
2. Does the shared corpus still pass against this copy? The generated schemas
   and fixture corpus are re-validated in place.

The consumer's own codec is exercised by its gate command (`--run-gate`), which
is what the registry records per consumer.
"""

from __future__ import annotations

import argparse
import json
import shlex
import subprocess
import sys
from pathlib import Path

import runtime_interactions_contract as core

CONTRACT = "oqto.runtime-interactions"


class Drift(Exception):
    """The vendored bundle is not the pinned bundle."""


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise Drift(message)


def _pin(bundle: Path, source_path: Path | None) -> tuple[dict, str]:
    manifest_path = bundle / "manifest.json"
    _require(manifest_path.is_file(), f"missing {manifest_path}")
    manifest_bytes = manifest_path.read_bytes()
    manifest = json.loads(manifest_bytes)
    actual = core.sha256(manifest_bytes)
    if source_path is None:
        _require(
            isinstance(manifest.get("scope"), str),
            "bundle has no manifest scope",
        )
        return manifest, actual
    _require(source_path.is_file(), f"missing {source_path}")
    source = json.loads(source_path.read_bytes())
    _require(source.get("contract") == CONTRACT, "SOURCE.json names another contract")
    _require(source.get("version") == 1, "SOURCE.json names another version")
    _require(
        source.get("manifest_sha256") == actual,
        f"manifest hash {actual} is not the pinned {source.get('manifest_sha256')}"
        " (edited or regenerated copy)",
    )
    _require(
        source.get("file_count") == len(manifest.get("files", {})) + 1,
        "SOURCE.json file_count disagrees with the manifest",
    )
    _require(
        source.get("input_count") == len(manifest.get("inputs", {})),
        "SOURCE.json input_count disagrees with the manifest",
    )
    return source, actual


def verify(
    bundle: Path,
    source_path: Path | None,
    core_root: Path,
    core_ref: str | None = None,
    skip_core: bool = False,
) -> dict:
    """Raise Drift on any divergence; return a receipt on success."""
    source, manifest_hash = _pin(bundle, source_path)
    ref = core_ref or (source.get("core_commit") if isinstance(source, dict) else None)
    core_path = source.get("core_path") if isinstance(source, dict) else None
    vendored_to = source.get("vendored_to") if isinstance(source, dict) else None
    receipt_path = {
        "core_path": core_path,
        "vendored_to": vendored_to,
    }
    if not skip_core:
        _require(
            isinstance(ref, str) and len(ref) == 40,
            "SOURCE.json has no full immutable core_commit",
        )
        try:
            core.verify_files(bundle, core_root, ref)
        except (ValueError, subprocess.CalledProcessError) as error:
            raise Drift(f"not the pinned Core bytes: {error}") from error
    counts = core.verify_corpus(bundle)
    return {
        **receipt_path,
        "bundle": str(bundle),
        "manifest_sha256": manifest_hash,
        "core_commit": ref,
        "files": len(json.loads((bundle / "manifest.json").read_bytes())["files"]) + 1,
        "counts": counts,
        "core_bytes_verified": not skip_core,
    }


def run_gate(command: str, cwd: Path) -> tuple[bool, str]:
    """Run the consumer's own codec gate; its exit code is authoritative."""
    completed = subprocess.run(
        shlex.split(command),
        cwd=str(cwd),
        capture_output=True,
        text=True,
        check=False,
    )
    tail = "\n".join((completed.stdout + completed.stderr).strip().splitlines()[-12:])
    return completed.returncode == 0, tail


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True, help="vendored v1 directory")
    parser.add_argument(
        "--source",
        type=Path,
        help="SOURCE.json (default: sibling ../SOURCE.json when present)",
    )
    parser.add_argument("--core", type=Path, default=core.ROOT, help="Core checkout")
    parser.add_argument("--core-ref", help="override the pinned commit (diagnostics only)")
    parser.add_argument("--skip-core", action="store_true", help="pin+bundle checks only")
    parser.add_argument("--run-gate", help="consumer gate command, e.g. 'bun test'")
    parser.add_argument("--gate-cwd", type=Path, help="working directory for --run-gate")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    bundle = args.bundle.resolve()
    source = args.source
    if source is None:
        candidate = bundle.parent / "SOURCE.json"
        source = candidate if candidate.is_file() else None
    try:
        receipt = verify(bundle, source, args.core.resolve(), args.core_ref, args.skip_core)
    except Drift as drift:
        print(f"FAIL {bundle}: {drift}", file=sys.stderr)
        return 1

    gate_ok = None
    gate_tail = ""
    if args.run_gate:
        gate_ok, gate_tail = run_gate(args.run_gate, (args.gate_cwd or bundle).resolve())
        receipt["gate"] = {"command": args.run_gate, "ok": gate_ok}

    if args.json:
        print(json.dumps(receipt, indent=1))
    else:
        print(
            f"OK {receipt['bundle']}: {receipt['files']} files, manifest "
            f"{receipt['manifest_sha256'][:12]}, pin {str(receipt['core_commit'])[:12]}"
            f"{'' if receipt['core_bytes_verified'] else ' (pin only)'}"
        )
        print(f"   corpus: {receipt['counts']}")
        if gate_ok is not None:
            print(f"   gate: {'passed' if gate_ok else 'FAILED'} :: {args.run_gate}")
    if gate_ok is False:
        print(gate_tail, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
