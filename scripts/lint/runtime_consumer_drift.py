"""Cross-repository drift check for runtime-interaction consumers.

Reads `CONSUMERS.toml` from Core, then for every vendored consumer:

* fetches its remote read-only (no checkout, no worktree change),
* compares its `SOURCE.json` pin with Core's current qualified manifest,
* re-verifies the vendored bytes against the pinned Core commit,
* reports stale, divergent, missing or not-yet-adopted consumers.

Runs by hand today and as a nightly job later. Exit code 1 with `--check` when a
vendored consumer no longer matches Core.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tomllib
from pathlib import Path

import runtime_consumer_verify as verify
import runtime_interactions_contract as core

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_REGISTRY = (
    ROOT / "backend/crates/oqto-protocol/contracts/runtime-interactions/CONSUMERS.toml"
)


def git(repo: Path, *args: str, check: bool = False) -> str:
    completed = subprocess.run(
        ["git", "-C", str(repo), *args],
        capture_output=True,
        text=True,
        check=False,
    )
    if check and completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or f"git {' '.join(args)} failed")
    return completed.stdout.strip()


def hooks_state(repo: Path) -> dict:
    """Whether this checkout would actually run .githooks/pre-commit."""
    configured = git(repo, "config", "--get", "core.hooksPath")
    hook = repo / ".githooks/pre-commit"
    present = hook.is_file()
    enabled = False
    if present and configured:
        candidate = Path(configured)
        candidate = candidate if candidate.is_absolute() else repo / candidate
        enabled = candidate.resolve() == (repo / ".githooks").resolve()
    return {
        "hooks_path": configured or None,
        "hook_present": present,
        "hook_executable": present and os.access(hook, os.X_OK),
        "verdict": "enabled" if enabled and os.access(hook, os.X_OK) else (
            "present-not-enabled" if present else "missing"
        ),
    }


def core_state(registry: dict) -> dict:
    bundle = ROOT / registry["core_path"]
    manifest = bundle / "manifest.json"
    if not manifest.is_file():
        raise SystemExit(f"core bundle missing: {manifest}")
    dirty = bool(git(ROOT, "status", "--porcelain", "--", str(bundle)))
    return {
        "path": registry["core_path"],
        "head": git(ROOT, "rev-parse", "HEAD", check=True),
        "manifest_sha256": core.sha256(manifest.read_bytes()),
        "worktree_clean": not dirty,
        "hooks": hooks_state(ROOT),
    }


def consumer_state(
    entry: dict,
    repos_root: Path,
    core_state: dict,
    fetch: bool,
    core_root: Path = ROOT,
) -> dict:
    name = entry["name"]
    repo = repos_root / entry["repo"]
    row = {"name": name, "repo": str(repo), "declared": entry.get("state", "planned")}
    if not (repo / ".git").exists():
        row["verdict"] = "missing-checkout"
        return row
    if fetch:
        git(repo, "fetch", "--quiet", "origin")
    row["repo_head"] = git(repo, "rev-parse", "--short", "HEAD")
    row["hooks"] = hooks_state(repo)["verdict"]
    if entry.get("state") != "vendored":
        row["verdict"] = "not-adopted"
        row["note"] = entry.get("note", "")
        return row

    bundle = repo / entry["bundle"]
    if not bundle.is_dir():
        row["verdict"] = "missing-bundle"
        return row
    source_path = repo / entry.get("source", str(Path(entry["bundle"]).parent / "SOURCE.json"))
    try:
        receipt = verify.verify(bundle, source_path, core_root)
    except verify.Drift as drift:
        row["verdict"] = "divergent"
        row["error"] = str(drift)
        return row

    row["pinned_commit"] = receipt["core_commit"]
    row["manifest_sha256"] = receipt["manifest_sha256"]
    if receipt["manifest_sha256"] != core_state["manifest_sha256"]:
        row["verdict"] = "stale"
        row["behind"] = git(
            ROOT, "rev-list", "--count", f"{receipt['core_commit']}..HEAD"
        ) or "unknown"
    else:
        row["verdict"] = "current"
    return row


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", type=Path, default=DEFAULT_REGISTRY)
    parser.add_argument(
        "--repos-root",
        type=Path,
        default=Path.home() / "byteowlz",
        help="directory holding sibling consumer checkouts",
    )
    parser.add_argument("--no-fetch", action="store_true", help="skip git fetch")
    parser.add_argument("--check", action="store_true", help="exit 1 on stale/divergent")
    parser.add_argument(
        "--require-hooks",
        action="store_true",
        help="also fail when a vendored checkout has no enabled pre-commit hook",
    )
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    registry = tomllib.loads(args.registry.read_text())
    state = core_state(registry)
    rows = [
        consumer_state(entry, args.repos_root, state, fetch=not args.no_fetch)
        for entry in registry.get("consumer", [])
    ]
    problems = [row for row in rows if row["verdict"] in {"stale", "divergent", "missing-bundle"}]

    if args.json:
        print(json.dumps({"core": state, "consumers": rows}, indent=1))
    else:
        print(f"core {state['head'][:12]} manifest {state['manifest_sha256'][:12]}"
              f" hooks={state['hooks']['verdict']}"
              f"{'' if state['worktree_clean'] else ' (DIRTY bundle)'}")
        for row in rows:
            detail = row.get("error") or row.get("note") or ""
            pin = row.get("manifest_sha256", "")[:12]
            suffix = f" pin={pin}" if pin else ""
            behind = f" behind={row['behind']} commits" if row.get("behind") else ""
            hooks = row.get("hooks", "-")
            print(
                f"  {row['name']:<20} {row['verdict']:<17} hooks={hooks:<21}"
                f"{suffix}{behind} {detail}".rstrip()
            )
    hooked_problems = [
        row
        for row in rows
        if row["declared"] == "vendored" and row.get("hooks") != "enabled"
    ]
    if args.require_hooks and hooked_problems:
        problems += hooked_problems
    if args.check and (problems or not state["worktree_clean"]):
        print(
            f"drift: {len(problems)} consumer(s) need attention",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
