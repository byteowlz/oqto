#!/usr/bin/env python3
"""One preserve-first managed extension reconciler for sync and provisioning.

Run as the home owner. Only recognized managed indexes may be retired/replaced;
unknown directories stay untouched. Every change has an owner-preserving archive
and a rollback journal outside Pi autoload. This does not claim RPC readiness.
"""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import uuid


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(65536), b""):
            checksum.update(chunk)
    return checksum.hexdigest()


def index_digest(path):
    index = path / "index.ts"
    return digest(index) if index.is_file() else None


def same_tree(left, right):
    def entries(root):
        result = {}
        for path in root.rglob("*"):
            relative = path.relative_to(root).as_posix()
            if relative == "install.sh":
                continue
            if path.is_symlink():
                result[relative] = ("link", os.readlink(path))
            elif path.is_file():
                result[relative] = ("file", digest(path))
        return result

    return entries(left) == entries(right)


def reconcile(home, source, names, policy):
    home, source = Path(home), Path(source)
    if policy.get("version") != 1:
        raise ValueError("unsupported managed extension policy")
    if home.stat().st_uid != os.geteuid():
        raise PermissionError("extension reconciliation must run as the home owner")
    if len(names) != len(set(names)) or any(
        not n or Path(n).name != n or n in {".", ".."} for n in names
    ):
        raise ValueError("invalid canonical extension names")
    # Validate the whole source before retiring any installed asset.
    for name in names:
        if not (source / name / "index.ts").is_file():
            raise ValueError(f"required extension missing from source: {name}")
    agent = home / ".pi/agent"
    for part in [
        home / ".pi",
        agent,
        agent / "extensions",
        agent / "extension-backups",
    ]:
        if part.is_symlink():
            raise ValueError(f"refuse symlinked managed destination: {part}")
        part.mkdir(mode=0o700, exist_ok=True)
    destination = agent / "extensions"
    lock_fd = os.open(
        agent / ".managed-extension-sync.lock",
        os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW,
        0o600,
    )
    with os.fdopen(lock_fd, "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        return reconcile_locked(agent, destination, source, names, policy)


def reconcile_locked(agent, destination, source, names, policy):
    retire, replace, unknown = [], [], []
    for name, rule in policy["supersessions"].items():
        if Path(name).name != name or name in {".", ".."}:
            raise ValueError("invalid superseded extension name")
        path = destination / name
        if not path.exists() and not path.is_symlink():
            continue
        replacement = rule["replacement"]
        if replacement is not None and replacement not in names:
            continue
        if index_digest(path) in rule["index_sha256"]:
            retire.append(name)
        else:
            unknown.append(name)
    for name in names:
        path = destination / name
        if path.exists() or path.is_symlink():
            if not path.is_symlink() and same_tree(source / name, path):
                continue
            known = set(policy.get("managed_index_sha256", {}).get(name, []))
            known.add(index_digest(source / name))
            if index_digest(path) not in known:
                raise ValueError(
                    f"unknown user-owned extension at {path}; preserved; reconcile manually before deployment"
                )
        replace.append(name)
    if not retire and not replace:
        return {
            "version": 1,
            "changed": False,
            "unknown_preserved": unknown,
            "archived": [],
            "installed": [],
        }
    backup = agent / "extension-backups" / ("managed-" + uuid.uuid4().hex)
    staging = Path(tempfile.mkdtemp(prefix=".extension-stage-", dir=agent))
    moved, installed = [], []
    try:
        # Staging is outside the discovery directory and completes before mutation.
        for name in replace:
            shutil.copytree(source / name, staging / name, symlinks=True)
            (staging / name / "install.sh").unlink(missing_ok=True)
        backup.mkdir(mode=0o700, parents=True)
        for name in retire + replace:
            path = destination / name
            if path.exists() or path.is_symlink():
                os.rename(path, backup / name)
                moved.append(name)
        for name in replace:
            os.rename(staging / name, destination / name)
            installed.append(name)
        receipt = {
            "version": 1,
            "changed": True,
            "backup": str(backup),
            "unknown_preserved": unknown,
            "archived": moved,
            "installed": installed,
            "owner_uid": os.geteuid(),
        }
        (backup / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        return receipt
    except BaseException:
        # Keep new bytes in the private staging directory on failure; restore exact old paths.
        for name in reversed(installed):
            os.rename(destination / name, staging / name)
        for name in reversed(moved):
            os.rename(backup / name, destination / name)
        raise
    finally:
        shutil.rmtree(staging)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--home", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--extensions-json", required=True)
    parser.add_argument("--policy-file")
    parser.add_argument("--policy-json")
    args = parser.parse_args()
    if args.policy_json:
        policy = json.loads(args.policy_json)
    else:
        policy = json.loads(Path(args.policy_file).read_text())
    try:
        print(
            json.dumps(
                reconcile(
                    args.home, args.source, json.loads(args.extensions_json), policy
                )
            )
        )
    except (OSError, ValueError) as error:
        print(json.dumps({"version": 1, "ready": False, "error": str(error)}))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
