#!/usr/bin/env python3
"""Run a source deployment from one private, independently checked-out commit.

Only committed Git objects are copied. The caller's ignored build outputs,
node_modules, sibling checkouts and mutable dependency staging are never reused.
Artifacts and the build log survive; owned source/build scratch is removed after
all deployment consumers have exited. This is build isolation, not a sandbox.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
from pathlib import Path


class SnapshotError(Exception):
    pass


def git(root: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-c", "core.hooksPath=/dev/null", "-C", str(root), *args],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def run(source: Path, config: Path, args: list[str]) -> int:
    source = source.resolve(strict=True)
    config = config.resolve(strict=True)
    # Refuse even --dev rather than silently losing uncommitted edits. Dirty
    # prebuilt artifacts remain a separate explicit installation policy.
    if git(source, "status", "--porcelain", "--untracked-files=normal"):
        raise SnapshotError(
            "source snapshot requires a clean checkout: uncommitted or untracked "
            "changes exist; commit them or deploy an explicitly selected --artifact"
        )
    commit = git(source, "rev-parse", "--verify", "HEAD^{commit}")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise SnapshotError("unexpected source commit identity")
    state = Path(os.environ.get("XDG_STATE_HOME", str(Path.home() / ".local/state")))
    builds = state / "oqto/deploy-builds"
    builds.mkdir(parents=True, exist_ok=True)
    run_dir = Path(tempfile.mkdtemp(prefix=f"{commit[:12]}-", dir=builds)).resolve()
    checkout = run_dir / "source"
    scratch = run_dir / "scratch"
    artifacts = run_dir / "artifacts"
    scratch.mkdir()
    artifacts.mkdir()
    print(f"[deploy] Frozen source {commit}; build record: {run_dir}", flush=True)
    try:
        # --no-local prevents hardlink/shared-object dependencies on the caller's
        # Git directory. A caller checkout/commit/GC cannot change this checkout.
        subprocess.run(
            [
                "git",
                "clone",
                "--quiet",
                "--no-local",
                "--no-checkout",
                "--",
                str(source),
                str(checkout),
            ],
            check=True,
        )
        git(checkout, "checkout", "--quiet", "--detach", commit)
        if git(checkout, "rev-parse", "HEAD") != commit:
            raise SnapshotError("snapshot checkout did not retain the captured commit")
        operator_config = run_dir / "hosts.toml"
        shutil.copyfile(config, operator_config)
        operator_config.chmod(0o600)
        env = dict(os.environ)
        env.update(
            OQTO_DEPLOY_SNAPSHOT_ROOT=str(checkout),
            OQTO_DEPLOY_SNAPSHOT_COMMIT=commit,
            OQTO_DEPLOY_ARTIFACT_DIR=str(artifacts),
            CARGO_TARGET_DIR=str(scratch / "cargo-target"),
            OQTO_DIST_CACHE_DIR=str(scratch / "dependency-sources"),
            OQTO_TEMPLATES_REPO=str(scratch / "dependency-sources/oqto-templates"),
            PI_AGENT_EXTENSIONS_REPO=str(
                scratch / "dependency-sources/pi-agent-extensions"
            ),
        )
        command = [
            "bash",
            str(checkout / "scripts/deploy.sh"),
            *args,
            "--config",
            str(operator_config),
            "--no-extension-update",
        ]
        with (run_dir / "build.log").open("wb") as log:
            child = subprocess.Popen(
                command,
                cwd=checkout,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )

            def forward(signum: int, _frame: object) -> None:
                # Wait for the child's own lease/activation cleanup before
                # deleting its source. Never kill unrelated services/processes.
                try:
                    child.send_signal(signum)
                except ProcessLookupError:
                    pass

            previous = {
                sig: signal.signal(sig, forward)
                for sig in (signal.SIGINT, signal.SIGTERM)
            }
            try:
                if child.stdout is None:
                    raise SnapshotError("build output pipe was not created")
                for line in child.stdout:
                    log.write(line)
                    log.flush()
                    sys.stdout.buffer.write(line)
                    sys.stdout.buffer.flush()
                result = child.wait()
            finally:
                # A logging/output error must not remove a still-running build.
                if child.poll() is None:
                    child.terminate()
                    child.wait()
                if child.stdout is not None:
                    child.stdout.close()
                for sig, handler in previous.items():
                    signal.signal(sig, handler)
        return result if result >= 0 else 128 - result
    finally:
        shutil.rmtree(checkout, ignore_errors=True)
        shutil.rmtree(scratch, ignore_errors=True)
        print(f"[deploy] Build record and artifacts retained at {run_dir}", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("args", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    forwarded = args.args[1:] if args.args[:1] == ["--"] else args.args
    os.umask(0o077)
    try:
        return run(args.source, args.config, forwarded)
    except (SnapshotError, OSError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
