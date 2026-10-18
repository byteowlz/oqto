#!/usr/bin/env python3
"""Read-only check of canonical main and backend source on two machines."""
import argparse
import json
import pathlib
import shlex
import subprocess
import sys

# Run exactly the same probe locally and remotely. No files, refs or index are written.
PROBE = r'''
import json, os, subprocess, sys
repo = os.path.expanduser(sys.argv[1])
def git(*args):
    return subprocess.check_output(["git", "--no-optional-locks", "-C", repo, *args])
def ref(name):
    return git("rev-parse", "--verify", name).decode().strip()
print(json.dumps({
    "main": ref("refs/heads/main"),
    "head": ref("HEAD"),
    "main_backend": ref("refs/heads/main:backend"),
    "head_backend": ref("HEAD:backend"),
    "backend_dirty": bool(git("status", "--porcelain=v1", "-z", "--untracked-files=all", "--", "backend")),
}))
'''


def compare(local, remote):
    """A UI branch may differ, but must contain precisely canonical backend source."""
    failures = []
    if local["main"] != remote["main"]:
        failures.append("Canonical main commit differs")
    if local["main_backend"] != remote["main_backend"]:
        failures.append("Canonical backend tree differs")
    for label, probe in [("local", local), ("remote", remote)]:
        if probe["head_backend"] != probe["main_backend"]:
            failures.append(f"{label}: checked-out backend differs from canonical main")
        if probe["backend_dirty"]:
            failures.append(f"{label}: tracked or untracked backend source is uncommitted")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ssh", required=True, help="SSH destination, e.g. mac")
    parser.add_argument("--remote-repo", required=True, help="Remote checkout; ~/ is expanded remotely")
    parser.add_argument("--local-repo", default=str(pathlib.Path(__file__).resolve().parents[2]))
    args = parser.parse_args()
    try:
        local = json.loads(subprocess.check_output(
            [sys.executable, "-c", PROBE, str(pathlib.Path(args.local_repo).resolve())], timeout=20))
        command = shlex.join(["python3", "-c", PROBE, args.remote_repo])
        remote = json.loads(subprocess.check_output(
            ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5", "--", args.ssh, command], timeout=25))
        failures = compare(local, remote)
        print(json.dumps({"version": 1, "passed": not failures, "failures": failures,
                          "local": local, "remote": remote}, indent=2))
        return int(bool(failures))
    except (OSError, subprocess.SubprocessError, ValueError, KeyError) as error:
        print(json.dumps({"version": 1, "passed": False, "error": str(error)}))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
