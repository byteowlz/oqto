"""Fast, scoped contract check for the pre-commit hook.

Runs only when the staged change touches the runtime-interaction contract, its
generated bundle, the consumer crate or the verifier tooling. Two cheap,
fail-closed questions: are the bundle bytes internally consistent and unedited,
and does the shared corpus still pass?

The expensive half (regenerate the bundle and diff, plus the TypeScript shape
gate) belongs to the push gate, `just check-runtime-interactions`.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

import runtime_interactions_contract as core

ROOT = core.ROOT

# A staged path matching any of these means the contract could have changed.
TRIGGERS = (
    "backend/crates/oqto-protocol/contracts/runtime-interactions/",
    "backend/crates/oqto-protocol/src/runtime_interactions",
    "backend/crates/oqto-protocol/examples/export_runtime_interactions.rs",
    "backend/crates/oqto-runner-client/",
    "scripts/lint/runtime_",
    "runtime-interactions/",
)


def staged_paths() -> list[str]:
    completed = subprocess.run(
        ["git", "-C", str(ROOT), "diff", "--cached", "--name-only", "-z", "--diff-filter=ACMR"],
        capture_output=True,
        check=True,
    )
    return [path for path in completed.stdout.decode().split("\0") if path]


def touches_contract(paths: list[str]) -> bool:
    return any(
        path.startswith(trigger) or "/runtime-interactions/" in path or path.endswith("/SOURCE.json")
        for path in paths
        for trigger in TRIGGERS
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, default=ROOT / core.BUNDLE)
    parser.add_argument("--paths", nargs="*", help="override staged paths (tests)")
    args = parser.parse_args()

    paths = args.paths if args.paths is not None else staged_paths()
    if not touches_contract(paths):
        print("runtime contract: no contract files staged")
        return 0

    bundle = args.bundle
    try:
        core.verify_files(bundle, ROOT, None)
        counts = core.verify_corpus(bundle)
    except ValueError as error:
        print(f"runtime contract: FAILED {error}", file=sys.stderr)
        print(
            "  Edited generated files are not allowed. Regenerate with:\n"
            "    just export-runtime-interactions /tmp/rt && diff -ru "
            f"{core.BUNDLE} /tmp/rt",
            file=sys.stderr,
        )
        return 1

    print(f"runtime contract: ok ({counts['request']} requests, "
          f"{counts['invalid']} refusals, {counts.get('source_pairs', 0)} source pairs)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
