#!/usr/bin/env python3
"""Qualify the newest pi-agent-extensions and promote the pin only if it works.

Takes the head of the extension repository (default: origin main), checks it in
an isolated clone, and only when every gate passes rewrites the
`pi-extensions` pin in dependencies.toml plus the managed-extension policy.

Gates:
  required   every canonical default extension exists at the candidate
  unit       the repository's own tests for those extensions (bun test)
  load       every extension loads and starts a session in the pinned Pi
             (scripts/dist/test-shipped-extensions.mjs, disposable HOME)

The policy's known index digests are regenerated from the candidate's git
history and merged with the existing ones (never shrunk), so any official
earlier version a user installed (e.g. via `just install`) is recognized and
upgraded, while real local edits still stop the sync.

A failed gate leaves the pin and policy untouched and exits non-zero.
Never touches ~/.pi; the clone, install and Pi HOME are disposable.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "dependencies.toml"
POLICY = ROOT / "scripts/dist/pi-extension-supersessions.json"
DEFAULTS = ROOT / "scripts/dist/pi-default-extensions.txt"
LOAD_TEST = ROOT / "scripts/dist/test-shipped-extensions.mjs"
REPO = "https://github.com/byteowlz/pi-agent-extensions.git"
PIN = re.compile(r'^(pi-extensions\s*=\s*")([0-9a-f]{40})(".*)$', re.MULTILINE)


class GateFailed(Exception):
    pass


def log(message: str) -> None:
    print(f"[pi-extensions-update] {message}", file=sys.stderr, flush=True)


def git(repo: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def git_bytes(repo: Path, *args: str) -> bytes:
    return subprocess.run(
        ["git", "-C", str(repo), *args], check=True, capture_output=True
    ).stdout


def default_extensions(path: Path | None = None) -> list[str]:
    path = path or DEFAULTS
    return [
        line.strip()
        for line in path.read_text().splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]


def current_pin(manifest: Path | None = None) -> str:
    manifest = manifest or MANIFEST
    match = PIN.search(manifest.read_text())
    if not match:
        raise ValueError(f"no pi-extensions pin in {manifest}")
    return match.group(2)


def history_digests(repo: Path, revision: str, name: str) -> set[str]:
    """sha256 of every committed <name>/index.ts reachable from `revision`."""
    commits = git(repo, "log", "--format=%H", revision, "--", f"{name}/index.ts")
    found = set()
    for commit in commits.split():
        try:
            blob = git_bytes(repo, "show", f"{commit}:{name}/index.ts")
        except subprocess.CalledProcessError:
            continue  # the commit deleted or renamed the file
        found.add(hashlib.sha256(blob).hexdigest())
    return found


def merged_policy(policy: dict, repo: Path, revision: str, names: list[str]) -> dict:
    updated = json.loads(json.dumps(policy))
    managed = updated.setdefault("managed_index_sha256", {})
    for name in names:
        known = set(managed.get(name, [])) | history_digests(repo, revision, name)
        managed[name] = sorted(known)
    updated["source_revision"] = revision
    return updated


def rewrite_pin(text: str, revision: str, note: str) -> str:
    def replace(match: re.Match) -> str:
        return f'{match.group(1)}{revision}"  # {note}'

    updated, count = PIN.subn(replace, text, count=1)
    if count != 1:
        raise ValueError("pi-extensions pin not found")
    return updated


def run_gate(name: str, argv: list[str], cwd: Path, timeout: int, log_dir: Path):
    log(f"gate {name}: {' '.join(argv[:3])} ...")
    started = time.monotonic()
    try:
        done = subprocess.run(
            argv, cwd=cwd, capture_output=True, text=True, timeout=timeout, check=False
        )
    except subprocess.TimeoutExpired as error:
        raise GateFailed(f"{name}: timed out after {timeout}s") from error
    output = done.stdout + done.stderr
    (log_dir / f"{name}.log").write_text(output)
    if done.returncode != 0:
        tail = "\n".join(output.strip().splitlines()[-15:])
        raise GateFailed(f"{name}: exit {done.returncode}\n{tail}")
    log(f"gate {name}: pass ({time.monotonic() - started:.0f}s)")
    return {"gate": name, "seconds": round(time.monotonic() - started, 1)}


def active_pi() -> Path:
    oqto = Path(shutil.which("oqto") or "/usr/local/bin/oqto").resolve()
    return oqto.parent.parent / "pi-runtime/pi/pi"


def pinned_pi_version(manifest: Path | None = None) -> str:
    manifest = manifest or MANIFEST
    match = re.search(r'^pi\s*=\s*"([^"]+)"', manifest.read_text(), re.MULTILINE)
    if not match:
        raise ValueError("no pi pin in dependencies.toml")
    return match.group(1)


def qualify(args) -> dict:
    names = default_extensions()
    work = Path(tempfile.mkdtemp(prefix="pi-extensions-candidate-"))
    try:
        repo = work / "repo"
        subprocess.run(
            ["git", "clone", "--quiet", args.repo, str(repo)],
            check=True,
            capture_output=True,
        )
        revision = git(repo, "rev-parse", f"{args.ref}^{{commit}}")
        pinned = current_pin()
        receipt = {
            "version": 1,
            "repository": args.repo,
            "ref": args.ref,
            "candidate": revision,
            "previous_pin": pinned,
            "gates": [],
        }
        log(f"candidate {revision[:9]} (pinned {pinned[:9]})")
        # Moving forward only: the candidate must contain the current pin.
        ancestry = subprocess.run(
            ["git", "-C", str(repo), "merge-base", "--is-ancestor", pinned, revision],
            capture_output=True,
            check=False,
        )
        if ancestry.returncode != 0:
            raise GateFailed(
                f"ancestry: candidate {revision[:9]} does not contain pin {pinned[:9]}"
            )
        missing = [n for n in names if not (repo / n / "index.ts").is_file()]
        if missing:
            raise GateFailed(f"required: missing at candidate: {', '.join(missing)}")
        receipt["gates"].append({"gate": "required", "extensions": names})

        logs = work / "logs"
        logs.mkdir()
        if not args.skip_unit:
            receipt["gates"].append(
                run_gate(
                    "install",
                    ["bun", "install", "--frozen-lockfile"],
                    repo,
                    600,
                    logs,
                )
            )
            receipt["gates"].append(
                run_gate(
                    "unit",
                    ["bun", "test", *[f"{n}/" for n in names]],
                    repo,
                    900,
                    logs,
                )
            )
        stage = work / "stage"
        stage.mkdir()
        for name in names:
            shutil.copytree(repo / name, stage / name, symlinks=True)
        pi = Path(args.pi) if args.pi else active_pi()
        version = args.pi_version or pinned_pi_version()
        if not pi.is_file():
            raise GateFailed(f"load: Pi binary not found: {pi} (pass --pi)")
        receipt["gates"].append(
            run_gate(
                "load",
                ["node", str(LOAD_TEST), str(pi), str(stage), version],
                ROOT,
                600,
                logs,
            )
        )
        receipt["pi"] = {"binary": str(pi), "version": version}
        receipt["policy"] = merged_policy(
            json.loads(POLICY.read_text()), repo, revision, names
        )
        return receipt
    finally:
        if args.keep_work:
            log(f"kept work directory {work}")
        else:
            shutil.rmtree(work, ignore_errors=True)


def promote(receipt: dict, commit: bool) -> bool:
    revision = receipt["candidate"]
    policy_before = POLICY.read_text()
    policy_after = json.dumps(receipt["policy"], indent=2) + "\n"
    manifest_before = MANIFEST.read_text()
    note = f"qualified {time.strftime('%Y-%m-%d')} by scripts/release/pi-extensions-update.py"
    manifest_after = rewrite_pin(manifest_before, revision, note)
    if policy_after == policy_before and manifest_after == manifest_before:
        log("pin and policy already current")
        return False
    if commit:
        dirty = git(ROOT, "status", "--porcelain", "--", str(MANIFEST), str(POLICY))
        if dirty:
            raise SystemExit(
                "refusing to commit: dependencies.toml or the policy has other "
                "uncommitted changes; rerun without --commit"
            )
    MANIFEST.write_text(manifest_after)
    POLICY.write_text(policy_after)
    log(f"promoted pi-extensions {receipt['previous_pin'][:9]} -> {revision[:9]}")
    if commit:
        message = (
            f"chore(deps): promote pi-extensions {revision[:9]} (qualified)\n\n"
            f"Gates: {', '.join(g['gate'] for g in receipt['gates'])} passed "
            f"against Pi {receipt['pi']['version']}. Known index digests "
            "regenerated from the extension history."
        )
        subprocess.run(
            [
                "git",
                "-C",
                str(ROOT),
                "commit",
                "-q",
                "-m",
                message,
                "--",
                str(MANIFEST),
                str(POLICY),
            ],
            check=True,
        )
        log(f"committed {git(ROOT, 'rev-parse', '--short', 'HEAD')}")
    return True


def write_receipt(receipt: dict) -> Path:
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))
    directory = state / "oqto/pi-extensions-qualification"
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}.json"
    slim = {k: v for k, v in receipt.items() if k != "policy"}
    path.write_text(json.dumps(slim, indent=2) + "\n")
    return path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--repo", default=REPO)
    parser.add_argument("--ref", default="origin/HEAD")
    parser.add_argument(
        "--pi", help="Pi binary for the load gate (default: active release)"
    )
    parser.add_argument(
        "--pi-version", help="expected Pi version (default: dependencies.toml)"
    )
    parser.add_argument(
        "--apply", action="store_true", help="promote when all gates pass"
    )
    parser.add_argument("--commit", action="store_true", help="commit the promotion")
    parser.add_argument("--skip-unit", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--keep-work", action="store_true")
    args = parser.parse_args()
    try:
        receipt = qualify(args)
    except GateFailed as error:
        log(f"NOT promoted; pin stays {current_pin()[:9]}: {error}")
        return 1
    except subprocess.CalledProcessError as error:
        log(f"NOT promoted; pin stays {current_pin()[:9]}: {error}")
        return 1
    receipt["promoted"] = promote(receipt, args.commit) if args.apply else False
    log(f"receipt {write_receipt(receipt)}")
    if not args.apply:
        log("all gates passed; rerun with --apply to promote")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
