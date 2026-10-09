#!/usr/bin/env python3
"""Root-side Oqto deployment and history-backfill transactions (ADR-0018, ADR-0056).

`activate` deploys one verified artifact through the canonical engine:
inventory the owned runners, refuse while sessions are busy, stop services,
take coherent byte snapshots of platform state, then run `oqto-setup install`
with this script's `check` as the post-activation hook. The hook restarts
services and proves health, version and runner executables *before*
`last-good` advances; any failure is rolled back by `oqto-setup` itself.

`backfill` reconciles oqto-log from Pi JSONL without touching binaries:
per principal it stops only that principal's writers, snapshots the store,
runs the requested `migrate-oqto-log` modes as the owner, verifies that Pi
JSONL was not modified, restores the exact snapshot on failure and restarts
the writers. It never writes Pi JSONL and never opens a user store as root.

Every run leaves a protected receipt directory under /root.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import pwd
import shutil
import socket
import stat
import subprocess
import sys
import time
import tomllib
import urllib.request
import uuid
from dataclasses import asdict, dataclass, field
from pathlib import Path

RELEASES = Path("/var/lib/oqto/releases")
RUNNER_SOCKETS = Path("/run/oqto/runner-sockets")
USERMGR_SOCKET = Path("/run/oqto/usermgr.sock")
TEAM_CONFIG = Path("/etc/oqto/config.toml")
SAFE_PATH = "/usr/local/bin:/usr/bin:/bin"
HEALTH_URL = "http://127.0.0.1:8080/api/health"
BACKFILL_MODES = ("sync-identities", "bootstrap", "validate", "validate-changed")
BUSY_STATES = {"starting", "streaming", "compacting", "aborting"}
STORE = Path(".local/share/oqto/oqto-log")
PI_SESSIONS = Path(".pi/agent/sessions")


class TransactionError(Exception):
    """A refused or failed transaction step; the message is operator-facing."""


@dataclass(frozen=True)
class Principal:
    name: str
    uid: int
    gid: int
    home: Path
    socket: Path

    def receipt(self) -> dict:
        return {key: str(value) for key, value in asdict(self).items()}


def utc_stamp() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def save(path: Path, obj: object) -> None:
    """Exclusive, private, durable JSON receipt."""
    with path.open("x") as handle:
        os.fchmod(handle.fileno(), 0o600)
        json.dump(obj, handle, indent=2, default=str)
        handle.flush()
        os.fsync(handle.fileno())


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def private_stage(root: Path, kind: str) -> Path:
    stage = root / f"oqto-{kind}-{utc_stamp()}-{uuid.uuid4().hex[:8]}"
    stage.mkdir(mode=0o700)
    return stage


def rpc(path: Path, kind: str, timeout: float = 35, **fields: object) -> dict:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(timeout)
        client.connect(str(path))
        client.sendall((json.dumps({"type": kind, **fields}) + "\n").encode())
        buffer = b""
        while b"\n" not in buffer:
            chunk = client.recv(65536)
            if not chunk:
                raise TransactionError(f"runner {path} closed without a reply")
            buffer += chunk
            if len(buffer) > 16 * 1024 * 1024:
                raise TransactionError(f"oversize runner reply from {path}")
    return json.loads(buffer.split(b"\n", 1)[0])


class SystemHost:
    """Real host operations; tests substitute a fake with the same surface."""

    def user_env(self, principal: Principal) -> dict[str, str]:
        runtime = f"/run/user/{principal.uid}"
        return {
            "HOME": str(principal.home),
            "USER": principal.name,
            "LOGNAME": principal.name,
            "PATH": SAFE_PATH,
            "XDG_RUNTIME_DIR": runtime,
            "DBUS_SESSION_BUS_ADDRESS": f"unix:path={runtime}/bus",
        }

    def run_as(
        self, principal: Principal, argv: list[str], timeout: float
    ) -> subprocess.CompletedProcess:
        env = [f"{key}={value}" for key, value in self.user_env(principal).items()]
        return subprocess.run(
            ["runuser", "-u", principal.name, "--", "env", "-i", *env, *argv],
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )

    def user_unit(self, principal: Principal, action: str, unit: str) -> None:
        result = self.run_as(principal, ["systemctl", "--user", action, unit], 120)
        if result.returncode != 0:
            raise TransactionError(
                f"{action} {unit} for {principal.name} failed: {result.stderr.strip()}"
            )

    def user_unit_exists(self, principal: Principal, unit: str) -> bool:
        result = self.run_as(
            principal,
            ["systemctl", "--user", "show", unit, "-p", "LoadState", "--value"],
            30,
        )
        return result.returncode == 0 and result.stdout.strip() not in ("", "not-found")

    def main_pid(self, principal: Principal, unit: str) -> int:
        result = self.run_as(
            principal,
            ["systemctl", "--user", "show", unit, "-p", "MainPID", "--value"],
            30,
        )
        return int(result.stdout.strip() or 0)

    def system_unit(self, action: str, unit: str) -> None:
        subprocess.run(["systemctl", action, unit], check=True, timeout=120)

    def system_unit_exists(self, unit: str) -> bool:
        result = subprocess.run(
            ["systemctl", "show", unit, "-p", "LoadState", "--value"],
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        return result.stdout.strip() not in ("", "not-found")

    def processes(self, uid: int, comm: str) -> list[int]:
        found = []
        for status in Path("/proc").glob("[0-9]*/status"):
            try:
                fields = dict(
                    line.split(":", 1)
                    for line in status.read_text().splitlines()
                    if ":" in line
                )
            except OSError:
                continue
            if (
                fields.get("Name", "").strip() == comm
                and int(fields.get("Uid", "-1").split()[0]) == uid
            ):
                found.append(int(status.parent.name))
        return found

    def exe(self, pid: int) -> Path:
        return Path(f"/proc/{pid}/exe").resolve(strict=True)

    def ping(self, principal: Principal) -> bool:
        try:
            return rpc(principal.socket, "ping", timeout=8).get("type") == "pong"
        except (OSError, ValueError, TransactionError):
            return False

    def sessions(self, principal: Principal) -> list[dict]:
        return rpc(principal.socket, "pi_list_sessions").get("sessions", [])


# ---------------------------------------------------------------- inventory


def principal_from_user(name: str, socket_path: Path) -> Principal:
    try:
        user = pwd.getpwnam(name)
    except KeyError as missing:
        raise TransactionError(f"unknown principal {name!r}") from missing
    if user.pw_uid == 0:
        raise TransactionError("root is never a runner principal")
    home = Path(user.pw_dir)
    if home.stat().st_uid != user.pw_uid:
        raise TransactionError(f"home {home} is not owned by {name}")
    return Principal(name, user.pw_uid, user.pw_gid, home, socket_path)


def personal_principal(name: str) -> Principal:
    uid = pwd.getpwnam(name).pw_uid
    return principal_from_user(name, Path(f"/run/user/{uid}/oqto-runner.sock"))


def team_principals(sockets: Path = RUNNER_SOCKETS) -> list[Principal]:
    """Every principal with a live managed runner socket; identity must agree."""
    principals = []
    for path in sorted(sockets.glob("*/oqto-runner.sock")):
        principal = principal_from_user(path.parent.name, path)
        meta = path.lstat()
        if not stat.S_ISSOCK(meta.st_mode) or meta.st_uid != principal.uid:
            raise TransactionError(
                f"runner socket {path} is not a socket owned by {principal.name}"
            )
        principals.append(principal)
    return principals


def select(principals: list[Principal], names: list[str] | None) -> list[Principal]:
    if not names:
        return principals
    by_name = {p.name: p for p in principals}
    unknown = sorted(set(names) - by_name.keys())
    if unknown:
        raise TransactionError(
            f"principals without a live managed runner: {', '.join(unknown)}"
        )
    return [by_name[name] for name in names]


def refuse_busy_sessions(
    host: SystemHost, principals: list[Principal]
) -> dict[str, list[dict]]:
    inventory, busy = {}, []
    for principal in principals:
        sessions = host.sessions(principal)
        inventory[principal.name] = sessions
        busy += [
            f"{principal.name}:{s.get('session_id')}={s.get('state')}"
            for s in sessions
            if s.get("state") in BUSY_STATES
        ]
    if busy:
        raise TransactionError("sessions are busy; retry when idle: " + ", ".join(busy))
    return inventory


# ---------------------------------------------------------------- activation


def snapshot_platform_state(stage: Path, homes: list[Path]) -> list[dict]:
    """Coherent byte copies of stopped-service SQLite and user config; never opens SQLite."""
    records = []
    root = stage / "state"
    root.mkdir(mode=0o700)
    for home in sorted(set(homes)):
        target = root / (home.name or "home")
        target.mkdir(mode=0o700)
        sources = [
            (home / ".local/share/oqto", "data", True),
            (home / ".config/oqto", "config", False),
        ]
        for area, label, only_db in sources:
            if not area.is_dir() or area.is_symlink():
                continue
            for path in sorted(area.rglob("*")):
                if path.is_symlink() or not path.is_file():
                    continue
                if only_db and not (
                    ".db" in path.name or path.suffix in (".sqlite", ".sqlite3")
                ):
                    continue
                records.append(
                    copy_preserving(path, target / label / path.relative_to(area))
                )
        for name in ("models.json", "settings.json", "auth.json"):
            path = home / ".pi/agent" / name
            if path.is_file() and not path.is_symlink():
                records.append(copy_preserving(path, target / "pi" / name))
    return records


def copy_preserving(path: Path, dest: Path) -> dict:
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, dest)
    meta = path.stat()
    os.chown(dest, meta.st_uid, meta.st_gid)
    return {
        "path": str(path),
        "copy": str(dest),
        "sha256": sha256(path),
        "uid": meta.st_uid,
        "gid": meta.st_gid,
        "mode": stat.S_IMODE(meta.st_mode),
    }


def stop_services(host: SystemHost, info: dict, principals: list[Principal]) -> None:
    if info["profile"] == "personal":
        for unit in ("oqto.service", "oqto-runner.service"):
            if host.user_unit_exists(principals[0], unit):
                host.user_unit(principals[0], "stop", unit)
        return
    host.system_unit("stop", "oqto.service")
    for principal in principals:
        host.user_unit(principal, "stop", "oqto-runner.service")


def restart_services(host: SystemHost, info: dict, principals: list[Principal]) -> None:
    """Ordered restart: usermgr -> runners -> control plane."""
    if info["profile"] == "personal":
        host.user_unit(principals[0], "restart", "oqto-runner.service")
        if host.user_unit_exists(principals[0], "oqto.service"):
            host.user_unit(principals[0], "restart", "oqto.service")
        return
    if host.system_unit_exists("oqto-usermgr.service"):
        host.system_unit("restart", "oqto-usermgr.service")
        wait_for(lambda: socket_accepts(USERMGR_SOCKET), 90, "oqto-usermgr socket")
    for principal in principals:
        host.user_unit(principal, "restart", "oqto-runner.service")
    host.system_unit("restart", "oqto.service")


def socket_accepts(path: Path) -> bool:
    try:
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(1)
            client.connect(str(path))
        return True
    except OSError:
        return False


def wait_for(probe, budget: float, what: str) -> None:
    deadline = time.monotonic() + budget
    while not probe():
        if time.monotonic() > deadline:
            raise TransactionError(f"{what} not ready after {budget:.0f}s")
        time.sleep(1)


def principals_from(info: dict) -> list[Principal]:
    return [
        Principal(
            p["name"], int(p["uid"]), int(p["gid"]), Path(p["home"]), Path(p["socket"])
        )
        for p in info["principals"]
    ]


def release_identity(release: Path) -> dict:
    manifest = tomllib.loads((release / "manifest.toml").read_text())["release"]
    return {"version": manifest["version"], "pi_version": manifest.get("pi_version")}


def health(url: str) -> dict | None:
    try:
        with urllib.request.urlopen(url, timeout=4) as response:
            return json.load(response)
    except (OSError, ValueError):
        return None


def check(stage: Path, host: SystemHost | None = None) -> None:
    """Post-activation hook: restart, then prove the *new* release serves."""
    host = host or SystemHost()
    info = json.loads((stage / "before.json").read_text())
    principals = principals_from(info)
    current = (RELEASES / "current").resolve(strict=True)
    identity = release_identity(current)
    restart_services(host, info, principals)

    reply: dict = {}

    def healthy() -> bool:
        nonlocal reply
        reply = health(info["health_url"]) or {}
        return (
            reply.get("status") == "ok" and reply.get("version") == identity["version"]
        )

    wait_for(
        healthy, float(info["health_budget"]), f"backend {identity['version']} health"
    )
    runner = (current / "immutable/bin/oqto-runner").resolve(strict=True)
    receipts = []
    for principal in principals:
        wait_for(lambda: host.ping(principal), 60, f"{principal.name} runner")
        pid = host.main_pid(principal, "oqto-runner.service")
        if pid <= 0 or host.exe(pid) != runner:
            raise TransactionError(f"{principal.name} runner is not executing {runner}")
        receipts.append(
            {"principal": principal.name, "pid": pid, "executable": str(runner)}
        )
    if info["profile"] == "personal":
        doctor = host.run_as(
            principals[0],
            ["oqtoctl", "doctor", "--contract", "--profile", "personal", "--strict"],
            300,
        )
        if doctor.returncode != 0:
            raise TransactionError(
                f"strict personal doctor failed: {doctor.stdout[-2000:]}{doctor.stderr[-2000:]}"
            )
    pi = current / "immutable/pi-runtime/pi/pi"
    if identity["pi_version"]:
        version = subprocess.run(
            [str(pi), "--version"],
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        ).stdout.strip()
        if version != identity["pi_version"]:
            raise TransactionError(
                f"bundled Pi reports {version!r}, release declares {identity['pi_version']}"
            )
    save(
        stage / "service-check.json",
        {"health": reply, "release": identity, "runners": receipts},
    )
    print("POST_ACTIVATE_CHECK_PASS")


def link_bundled_pi(entry: Path = Path("/usr/local/bin/pi")) -> str:
    """Point the shared `pi` entrypoint at the active release only when it is managed."""
    target = RELEASES / "current/immutable/pi-runtime/pi/pi"
    managed = ("/var/lib/oqto/",)
    if entry.is_symlink() and not os.readlink(entry).startswith(managed):
        return f"preserved unmanaged {entry} -> {os.readlink(entry)}"
    if entry.exists() and not entry.is_symlink():
        return f"preserved unmanaged regular file {entry}"
    staged = entry.with_name(f".oqto-pi-{uuid.uuid4().hex}")
    staged.symlink_to(target)
    os.replace(staged, entry)
    return f"linked {entry} -> {target}"


def activate(args: argparse.Namespace, host: SystemHost | None = None) -> Path:
    host = host or SystemHost()
    if os.geteuid() != 0:
        raise TransactionError("activate must run as root")
    stage = private_stage(args.receipt_root, "deploy")
    artifact = stage / args.artifact.name
    for source, dest, mode in (
        (args.setup, stage / "oqto-setup", 0o700),
        (args.artifact, artifact, 0o600),
        (args.checksum, stage / f"{args.artifact.name}.sha256", 0o600),
        (Path(__file__).resolve(), stage / "deploy-transaction.py", 0o700),
    ):
        if not stat.S_ISREG(source.lstat().st_mode):
            raise TransactionError(f"{source} is not a regular file")
        with source.open("rb") as src, dest.open("xb") as dst:
            os.fchmod(dst.fileno(), mode)
            shutil.copyfileobj(src, dst)
            dst.flush()
            os.fsync(dst.fileno())

    if args.profile == "personal":
        principals = [personal_principal(args.principal)]
        backend_home = principals[0].home
    else:
        principals = team_principals()
        backend_home = Path(pwd.getpwnam("oqto").pw_dir)
        if not principals:
            raise TransactionError("no managed runner sockets found")
    sessions = (
        {} if args.allow_busy_sessions else refuse_busy_sessions(host, principals)
    )
    current = RELEASES / "current"
    info = {
        "profile": args.profile,
        "previous": os.readlink(current) if current.is_symlink() else None,
        "principals": [p.receipt() for p in principals],
        "sessions": sessions,
        "health_url": args.health_url,
        "health_budget": args.health_budget,
        "artifact_sha256": sha256(artifact),
    }
    save(stage / "before.json", info)
    hook = stage / "service-check"
    hook.write_text(
        "#!/bin/sh\nexec /usr/bin/timeout "
        f"{int(args.health_budget) + 600} /usr/bin/python3 {stage / 'deploy-transaction.py'} check --stage {stage}\n"
    )
    hook.chmod(0o700)

    stop_services(host, info, principals)
    try:
        records = snapshot_platform_state(
            stage, [backend_home, *[p.home for p in principals]]
        )
        save(stage / "state-manifest.json", records)
        command = [
            str(stage / "oqto-setup"),
            "install",
            "--artifact",
            str(artifact),
            "--checksum",
            str(stage / f"{args.artifact.name}.sha256"),
            "--doctor-strict",
            "false",
            "--keep-releases",
            str(args.keep_releases),
            "--post-activate-check",
            str(hook),
        ]
        command += ["--allow-downgrade"] if args.allow_downgrade else []
        command += ["--allow-dirty-source"] if args.allow_dirty_source else []
        with (stage / "install.log").open("x") as log:
            os.fchmod(log.fileno(), 0o600)
            result = subprocess.run(
                command,
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=args.health_budget + 1800,
                check=False,
            )
        if result.returncode != 0:
            raise TransactionError(
                f"oqto-setup install failed; see {stage / 'install.log'}"
            )
        pi = link_bundled_pi()
        unchanged = all(
            sha256(Path(r["path"])) == r["sha256"]
            for r in records
            if r["path"].endswith(
                (".toml", "models.json", "settings.json", "auth.json")
            )
            and Path(r["path"]).is_file()
        )
        save(
            stage / "activated.json",
            {
                "current": os.readlink(current),
                "release": release_identity(current.resolve()),
                "principals": len(principals),
                "pi_entrypoint": pi,
                "configs_unchanged": unchanged,
            },
        )
    except BaseException as error:
        # oqto-setup already restored current/bins/frontend; bring services back.
        try:
            restart_services(host, info, principals)
        finally:
            save(
                stage / "failure.json",
                {
                    "error": str(error),
                    "current": os.readlink(current) if current.is_symlink() else None,
                },
            )
        raise
    print(f"ACTIVATED {stage}")
    return stage


# ---------------------------------------------------------------- backfill


def audit_store(principal: Principal) -> Path:
    """Metadata-only ownership audit; refuses foreign-owned or symlinked entries."""
    store = principal.home / STORE
    if not store.exists():
        return store
    for path in (store, *store.rglob("*")):
        meta = path.lstat()
        if stat.S_ISLNK(meta.st_mode):
            raise TransactionError(
                f"{principal.name}: refusing symlink in history store: {path}"
            )
        if meta.st_uid != principal.uid:
            raise TransactionError(
                f"{principal.name}: {path} is owned by uid {meta.st_uid}, not {principal.uid}"
            )
    return store


def tree_state(root: Path) -> dict[str, list]:
    """Content digest of every regular file below root (relative path keyed)."""
    if not root.exists():
        return {}
    return {
        str(path.relative_to(root)): [
            sha256(path),
            stat.S_IMODE(path.lstat().st_mode),
            path.lstat().st_uid,
        ]
        for path in sorted(root.rglob("*"))
        if path.is_file() and not path.is_symlink()
    }


def jsonl_state(home: Path) -> dict[str, list]:
    """Cheap identity of Pi-owned JSONL: any write changes size/mtime/inode."""
    root = home / PI_SESSIONS
    if not root.exists():
        return {}
    state = {}
    for path in sorted(root.rglob("*.jsonl")):
        meta = path.lstat()
        state[str(path.relative_to(root))] = [
            meta.st_size,
            meta.st_mtime_ns,
            meta.st_ino,
        ]
    return state


def copy_tree(source: Path, dest: Path) -> None:
    """Exact copy (bytes, modes, times, ownership) of a directory tree."""
    shutil.copytree(source, dest, symlinks=True, copy_function=shutil.copy2)
    for path in (dest, *dest.rglob("*")):
        original = source / path.relative_to(dest)
        meta = original.lstat()
        os.lchown(path, meta.st_uid, meta.st_gid)


@dataclass
class BackfillResult:
    principal: str
    status: str = "pending"
    modes: list[dict] = field(default_factory=list)
    error: str | None = None
    store_before: int = 0
    store_changed: bool = False
    restored: bool = False


def backfill_principal(
    host: SystemHost, principal: Principal, args: argparse.Namespace, stage: Path
) -> BackfillResult:
    result = BackfillResult(principal.name)
    store = audit_store(principal)
    personal = args.profile == "personal"
    # A personal backend reads the same store and can create WAL sidecars; stop
    # it before the runner it depends on, and start the runner first again.
    writers = (
        ["oqto.service"]
        if personal and host.user_unit_exists(principal, "oqto.service")
        else []
    ) + ["oqto-runner.service"]
    for unit in writers:
        host.user_unit(principal, "stop", unit)
    snapshot = stage / "snapshots" / principal.name
    try:
        if host.processes(principal.uid, "oqto-runner") or (
            personal and host.processes(principal.uid, "oqto")
        ):
            raise TransactionError(
                f"{principal.name}: history writers still running after stop"
            )
        before_jsonl = jsonl_state(principal.home)
        before_store = tree_state(store)
        result.store_before = len(before_store)
        if store.exists():
            copy_tree(store, snapshot)
        try:
            for mode in args.modes:
                argv = ["oqto"]
                if not personal and TEAM_CONFIG.is_file():
                    argv += ["--config", str(TEAM_CONFIG)]
                argv += ["runner", "migrate-oqto-log", "--mode", mode]
                started = time.monotonic()
                ran = host.run_as(principal, argv, args.mode_timeout)
                log = stage / "logs" / f"{principal.name}.{mode}.log"
                log.parent.mkdir(mode=0o700, exist_ok=True)
                log.write_text(ran.stdout[-200_000:] + ran.stderr[-200_000:])
                log.chmod(0o600)
                result.modes.append(
                    {
                        "mode": mode,
                        "exit": ran.returncode,
                        "seconds": round(time.monotonic() - started, 1),
                    }
                )
                if ran.returncode != 0:
                    raise TransactionError(
                        f"{principal.name}: {mode} failed (see {log})"
                    )
            if jsonl_state(principal.home) != before_jsonl:
                raise TransactionError(
                    f"{principal.name}: Pi JSONL changed during backfill; investigate before retrying"
                )
            audit_store(principal)
            result.store_changed = tree_state(store) != before_store
            result.status = "ok"
        except BaseException as error:
            result.error = str(error)
            result.status = "failed"
            restore_store(store, snapshot, stage / "failed" / principal.name)
            result.restored = True
            if tree_state(store) != before_store:
                result.error += "; RESTORE MISMATCH, inspect snapshot"
                result.restored = False
    finally:
        for unit in reversed(writers):
            host.user_unit(principal, "start", unit)
    if result.status == "ok" and not host.ping(principal):
        result.status, result.error = (
            "failed",
            f"{principal.name}: runner did not come back",
        )
    return result


def restore_store(store: Path, snapshot: Path, failed: Path) -> None:
    """Retain the failed state for diagnosis, then put the exact snapshot back."""
    if store.exists():
        failed.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(store), str(failed))
    if snapshot.exists():
        copy_tree(snapshot, store)


def backfill(
    args: argparse.Namespace,
    host: SystemHost | None = None,
    principals: list[Principal] | None = None,
) -> int:
    host = host or SystemHost()
    if principals is None:
        if os.geteuid() != 0:
            raise TransactionError("backfill must run as root")
        principals = (
            [personal_principal(args.principal)]
            if args.profile == "personal"
            else team_principals()
        )
    principals = select(principals, args.only)
    if not args.allow_busy_sessions:
        refuse_busy_sessions(host, principals)
    stage = private_stage(args.receipt_root, "backfill")
    results = []
    for principal in principals:
        try:
            results.append(backfill_principal(host, principal, args, stage))
        except TransactionError as refused:
            results.append(
                BackfillResult(principal.name, status="refused", error=str(refused))
            )
        print(
            f"{results[-1].principal}: {results[-1].status}"
            + (f" ({results[-1].error})" if results[-1].error else "")
        )
    save(
        stage / "backfill.json",
        {"modes": args.modes, "results": [asdict(r) for r in results]},
    )
    failed = [r.principal for r in results if r.status != "ok"]
    print(f"BACKFILL {'FAILED for ' + ', '.join(failed) if failed else 'OK'} {stage}")
    return 1 if failed else 0


# ---------------------------------------------------------------- CLI


def modes(value: str) -> list[str]:
    chosen = [mode.strip() for mode in value.split(",") if mode.strip()]
    unknown = sorted(set(chosen) - set(BACKFILL_MODES))
    if not chosen or unknown:
        raise argparse.ArgumentTypeError(
            f"modes must be from {', '.join(BACKFILL_MODES)}"
        )
    return chosen


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    commands = root.add_subparsers(dest="command", required=True)

    def common(sub: argparse.ArgumentParser) -> None:
        sub.add_argument("--profile", choices=("personal", "team"), required=True)
        sub.add_argument("--principal", help="personal profile: the owning account")
        sub.add_argument("--receipt-root", type=Path, default=Path("/root"))
        sub.add_argument("--allow-busy-sessions", action="store_true")

    act = commands.add_parser(
        "activate", help="install one verified artifact transactionally"
    )
    common(act)
    act.add_argument("--setup", type=Path, required=True)
    act.add_argument("--artifact", type=Path, required=True)
    act.add_argument("--checksum", type=Path, required=True)
    act.add_argument("--health-url", default=HEALTH_URL)
    act.add_argument("--health-budget", type=float, default=180)
    act.add_argument("--keep-releases", type=int, default=3)
    act.add_argument("--allow-downgrade", action="store_true")
    act.add_argument("--allow-dirty-source", action="store_true")

    hook = commands.add_parser(
        "check", help="post-activation hook (invoked by oqto-setup)"
    )
    hook.add_argument("--stage", type=Path, required=True)

    fill = commands.add_parser(
        "backfill", help="protected oqto-log backfill, no binary changes"
    )
    common(fill)
    fill.add_argument(
        "--modes", type=modes, default=["sync-identities", "bootstrap", "validate"]
    )
    fill.add_argument(
        "--only", action="append", help="restrict to these principals (repeatable)"
    )
    fill.add_argument("--mode-timeout", type=float, default=3600)
    return root


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    if getattr(args, "profile", None) == "personal" and not args.principal:
        print("error: --profile personal requires --principal", file=sys.stderr)
        return 2
    try:
        if args.command == "activate":
            activate(args)
        elif args.command == "check":
            check(args.stage)
        else:
            return backfill(args)
    except (TransactionError, OSError, subprocess.SubprocessError, KeyError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
