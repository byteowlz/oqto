#!/usr/bin/env python3
"""Regression tests for scripts/dist/deploy-transaction.py without root.

A fake host replaces systemd/runuser/proc; principals are the invoking user
with temporary homes, so ownership checks still run against real metadata.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import pwd
import socket
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/dist/deploy-transaction.py"
spec = importlib.util.spec_from_file_location("deploy_transaction", SCRIPT)
assert spec and spec.loader
tx = importlib.util.module_from_spec(spec)
sys.modules["deploy_transaction"] = tx
spec.loader.exec_module(tx)

ME = pwd.getpwuid(os.getuid())

FAKE_OQTO = r"""#!/usr/bin/env python3
# Fake `oqto runner migrate-oqto-log --mode X`: deterministic, idempotent store
# writes; behaviour switches are files in $HOME/.fake.
import os, sys
from pathlib import Path
home = Path(os.environ["HOME"]); mode = sys.argv[sys.argv.index("--mode") + 1]
store = home / ".local/share/oqto/oqto-log/abc123"; ctl = home / ".fake"
(ctl / "calls").open("a").write(mode + "\n")
if (ctl / f"fail-{mode}").exists():
    (store / "session.db").write_bytes(b"half-written")
    print("bootstrap-failure: synthetic", file=sys.stderr); sys.exit(1)
if (ctl / f"touch-jsonl-{mode}").exists():
    with (home / ".pi/agent/sessions/w/s.jsonl").open("a") as f: f.write('{"type":"x"}\n')
store.mkdir(parents=True, exist_ok=True)
(store / "session.db").write_bytes(b"imported-" + mode.encode() if mode != "validate" else (store / "session.db").read_bytes())
print(f"{mode} ok")
"""


class FakeHost(tx.SystemHost):
    def __init__(self, bin_dir: Path):
        self.bin_dir = bin_dir
        self.events: list[tuple[str, str]] = []
        self.running: set[str] = {"oqto-runner.service", "oqto.service"}
        self.stray: list[int] = []
        self.sessions_by_name: dict[str, list[dict]] = {}
        self.exe_path: Path | None = None
        self.doctor_exit = 0
        self.watchdog_active = False
        self.backend_running: list[int] = []

    def run_as(self, principal, argv, timeout):
        if argv[:1] == ["oqtoctl"]:
            return subprocess.CompletedProcess(argv, self.doctor_exit, "doctor", "")
        env = {"HOME": str(principal.home), "PATH": f"{self.bin_dir}:/usr/bin:/bin"}
        return subprocess.run(
            argv, env=env, capture_output=True, text=True, timeout=timeout, check=False
        )

    def user_unit(self, principal, action, unit):
        self.events.append((action, unit))
        (self.running.discard if action == "stop" else self.running.add)(unit)

    def user_unit_exists(self, principal, unit):
        return True

    def user_unit_active(self, principal, unit):
        assert unit == tx.WATCHDOG_TIMER
        return self.watchdog_active

    def system_unit_active(self, unit):
        assert unit == tx.WATCHDOG_TIMER
        return self.watchdog_active

    def system_unit(self, action, unit):
        self.events.append((f"system-{action}", unit))

    def system_unit_exists(self, unit):
        return False

    def processes(self, uid, comm):
        if comm == "oqto":
            return self.backend_running
        return self.stray if comm == "oqto-runner" else []

    def ping(self, principal):
        return "oqto-runner.service" in self.running

    def sessions(self, principal):
        return self.sessions_by_name.get(principal.name, [])

    def main_pid(self, principal, unit):
        return 4242

    def exe(self, pid):
        assert self.exe_path is not None
        return self.exe_path


def backfill_args(
    receipts: Path, profile: str = "team", **overrides
) -> argparse.Namespace:
    values = dict(
        profile=profile,
        principal=ME.pw_name,
        receipt_root=receipts,
        allow_busy_sessions=False,
        modes=["sync-identities", "bootstrap", "validate"],
        only=None,
        mode_timeout=60,
    )
    values.update(overrides)
    return argparse.Namespace(**values)


class Fixture(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.home = self.root / "home"
        self.receipts = self.root / "receipts"
        self.receipts.mkdir()
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        (bin_dir / "oqto").write_text(FAKE_OQTO)
        (bin_dir / "oqto").chmod(0o755)
        (self.home / ".fake").mkdir(parents=True)
        sessions = self.home / ".pi/agent/sessions/w"
        sessions.mkdir(parents=True)
        (sessions / "s.jsonl").write_text('{"type":"session","id":"01a0"}\n')
        store = self.home / ".local/share/oqto/oqto-log/abc123"
        store.mkdir(parents=True)
        (store / "session.db").write_bytes(b"original-bytes")
        (store / "session.db-wal").write_bytes(b"original-wal")
        self.host = FakeHost(bin_dir)
        self.principal = tx.Principal(
            ME.pw_name, ME.pw_uid, ME.pw_gid, self.home, self.root / "runner.sock"
        )

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def store(self) -> dict:
        return tx.tree_state(self.home / tx.STORE)

    def run_backfill(self, principal=None, **overrides) -> tuple[int, dict]:
        code = tx.backfill(
            backfill_args(self.receipts, **overrides),
            self.host,
            [principal or self.principal],
        )
        receipt = json.loads(
            next(self.receipts.glob("oqto-backfill-*/backfill.json")).read_text()
        )
        return code, receipt

    def calls(self) -> list[str]:
        path = self.home / ".fake/calls"
        return path.read_text().split() if path.exists() else []


class Backfill(Fixture):
    def test_owner_runs_modes_with_writers_stopped_and_jsonl_untouched(self) -> None:
        jsonl = tx.jsonl_state(self.home)
        code, receipt = self.run_backfill()
        self.assertEqual(code, 0, receipt)
        result = receipt["results"][0]
        self.assertEqual(result["status"], "ok")
        self.assertTrue(result["store_changed"])
        self.assertEqual(self.calls(), ["sync-identities", "bootstrap", "validate"])
        self.assertEqual(tx.jsonl_state(self.home), jsonl)
        self.assertEqual(
            self.host.events,
            [("stop", "oqto-runner.service"), ("start", "oqto-runner.service")],
        )
        snapshot = (
            next(self.receipts.glob("oqto-backfill-*/snapshots"))
            / ME.pw_name
            / "abc123/session.db"
        )
        self.assertEqual(snapshot.read_bytes(), b"original-bytes")
        self.assertEqual(
            oct(next(self.receipts.glob("oqto-backfill-*")).stat().st_mode & 0o777),
            "0o700",
        )

    def test_personal_backfill_also_quiesces_the_backend_reader(self) -> None:
        code, _ = self.run_backfill(profile="personal")
        self.assertEqual(code, 0)
        self.assertEqual(
            self.host.events,
            [
                ("stop", "oqto.service"),
                ("stop", "oqto-runner.service"),
                ("start", "oqto-runner.service"),
                ("start", "oqto.service"),
            ],
        )

    def test_second_run_is_idempotent(self) -> None:
        self.assertEqual(self.run_backfill()[0], 0)
        after_first = self.store()
        for receipt in self.receipts.glob("oqto-backfill-*"):
            receipt.rename(self.root / f"done-{receipt.name}")
        code, receipt = self.run_backfill()
        self.assertEqual(code, 0)
        self.assertFalse(receipt["results"][0]["store_changed"])
        self.assertEqual(self.store(), after_first)

    def test_failure_restores_exact_snapshot_and_keeps_failed_state(self) -> None:
        before = self.store()
        (self.home / ".fake/fail-bootstrap").touch()
        code, receipt = self.run_backfill()
        self.assertEqual(code, 1)
        result = receipt["results"][0]
        self.assertEqual((result["status"], result["restored"]), ("failed", True))
        self.assertEqual(self.store(), before)
        failed = (
            next(self.receipts.glob("oqto-backfill-*/failed"))
            / ME.pw_name
            / "abc123/session.db"
        )
        self.assertEqual(failed.read_bytes(), b"half-written")
        self.assertIn(("start", "oqto-runner.service"), self.host.events)
        self.assertEqual(self.calls(), ["sync-identities", "bootstrap"])

    def test_pi_jsonl_modification_fails_and_restores_the_store(self) -> None:
        before = self.store()
        (self.home / ".fake/touch-jsonl-sync-identities").touch()
        code, receipt = self.run_backfill()
        self.assertEqual(code, 1)
        self.assertIn("Pi JSONL changed", receipt["results"][0]["error"])
        self.assertEqual(self.store(), before)

    def test_foreign_owned_store_is_refused_before_any_stop_or_mode(self) -> None:
        stranger = tx.Principal(
            "stranger", ME.pw_uid + 1, ME.pw_gid, self.home, self.principal.socket
        )
        code, receipt = self.run_backfill(principal=stranger, allow_busy_sessions=True)
        self.assertEqual(code, 1)
        self.assertEqual(receipt["results"][0]["status"], "refused")
        self.assertIn("owned by uid", receipt["results"][0]["error"])
        self.assertEqual(self.host.events, [])
        self.assertEqual(self.calls(), [])

    def test_symlinked_store_entry_is_refused(self) -> None:
        (self.home / tx.STORE / "abc123/escape").symlink_to("/etc")
        code, receipt = self.run_backfill()
        self.assertEqual(code, 1)
        self.assertIn("symlink", receipt["results"][0]["error"])
        self.assertEqual(self.calls(), [])

    def test_lingering_writer_aborts_without_running_modes(self) -> None:
        before = self.store()
        self.host.stray = [1234]
        code, receipt = self.run_backfill()
        self.assertEqual(code, 1)
        self.assertIn("still running", receipt["results"][0]["error"])
        self.assertEqual(self.calls(), [])
        self.assertEqual(self.store(), before)
        self.assertEqual(self.host.events[-1], ("start", "oqto-runner.service"))

    def test_busy_sessions_refuse_the_whole_run(self) -> None:
        self.host.sessions_by_name[ME.pw_name] = [
            {"session_id": "oqto-1", "state": "streaming"}
        ]
        with self.assertRaises(tx.TransactionError):
            tx.backfill(backfill_args(self.receipts), self.host, [self.principal])
        self.assertEqual(self.host.events, [])

    def test_unknown_principals_and_destructive_modes_are_rejected(self) -> None:
        with self.assertRaises(tx.TransactionError):
            tx.select([self.principal], ["nobody"])
        for mode in ("rebuild-from-jsonl", "unsplit", ""):
            with self.assertRaises(argparse.ArgumentTypeError):
                tx.modes(mode)


class Activation(Fixture):
    def test_team_inventory_requires_owned_sockets(self) -> None:
        sockets = self.root / "sockets"
        live = sockets / ME.pw_name
        live.mkdir(parents=True)
        server = socket.socket(socket.AF_UNIX)
        server.bind(str(live / "oqto-runner.sock"))
        try:
            principals = tx.team_principals(sockets)
            self.assertEqual([p.name for p in principals], [ME.pw_name])
            (live / "oqto-runner.sock").unlink()
            (live / "oqto-runner.sock").write_text("not a socket")
            with self.assertRaises(tx.TransactionError):
                tx.team_principals(sockets)
        finally:
            server.close()

    def test_snapshot_copies_databases_configs_and_pi_settings_only(self) -> None:
        data = self.home / ".local/share/oqto"
        (data / "oqto.db").write_bytes(b"db")
        (data / "notes.txt").write_text("not state")
        config = self.home / ".config/oqto"
        config.mkdir(parents=True)
        (config / "config.toml").write_text("[x]\n")
        (config / "linked.toml").symlink_to(config / "config.toml")
        (self.home / ".pi/agent/models.json").write_text("{}")
        stage = self.root / "stage"
        stage.mkdir()
        records = tx.snapshot_platform_state(stage, [self.home, self.home])
        copied = sorted(Path(r["path"]).name for r in records)
        # History (oqto-log shards) is never part of the activation snapshot.
        self.assertEqual(copied, ["config.toml", "models.json", "oqto.db"])
        self.assertFalse(any("oqto-log" in r["path"] for r in records))
        for record in records:
            self.assertEqual(
                Path(record["copy"]).read_bytes(), Path(record["path"]).read_bytes()
            )

    def test_watchdog_is_paused_and_resumed_only_if_it_was_running(self) -> None:
        for profile, stop, start in (
            ("personal", ("stop", tx.WATCHDOG_TIMER), ("start", tx.WATCHDOG_TIMER)),
            (
                "team",
                ("system-stop", tx.WATCHDOG_TIMER),
                ("system-start", tx.WATCHDOG_TIMER),
            ),
        ):
            info = {"profile": profile}
            self.host.events.clear()
            self.host.watchdog_active = True
            self.assertTrue(tx.pause_watchdog(self.host, info, [self.principal]))
            tx.resume_watchdog(self.host, info, [self.principal])
            self.assertEqual(self.host.events, [stop, start])
            self.host.events.clear()
            self.host.watchdog_active = False
            self.assertFalse(tx.pause_watchdog(self.host, info, [self.principal]))
            self.assertEqual(self.host.events, [])

    def test_writers_restarted_during_the_window_abort_activation(self) -> None:
        info = {"backend_uid": ME.pw_uid}
        tx.require_writers_stopped(self.host, info, [self.principal])
        self.host.backend_running = [99]
        clock = iter([0.0, 0.0, 31.0])
        original_monotonic, original_sleep = tx.time.monotonic, tx.time.sleep
        tx.time.monotonic = lambda: next(clock)
        tx.time.sleep = lambda _: None
        try:
            with self.assertRaises(tx.TransactionError) as refused:
                tx.require_writers_stopped(self.host, info, [self.principal])
        finally:
            tx.time.monotonic, tx.time.sleep = original_monotonic, original_sleep
        self.assertIn("restarted while stopped", str(refused.exception))

    def test_pi_entrypoint_is_relinked_only_when_managed(self) -> None:
        entry = self.root / "pi"
        self.assertIn("linked", tx.link_bundled_pi(entry))
        self.assertEqual(
            os.readlink(entry), str(tx.RELEASES / "current/immutable/pi-runtime/pi/pi")
        )
        entry.unlink()
        entry.symlink_to("/home/someone/.bun/bin/pi")
        self.assertIn("preserved", tx.link_bundled_pi(entry))
        self.assertEqual(os.readlink(entry), "/home/someone/.bun/bin/pi")
        entry.unlink()
        entry.write_text("custom")
        self.assertIn("preserved", tx.link_bundled_pi(entry))

    def make_release(self, version: str = "0.5.9") -> Path:
        releases = self.root / "releases"
        release = releases / f"oqto-{version}-x86_64-unknown-linux-gnu"
        (release / "immutable/bin").mkdir(parents=True)
        (release / "immutable/bin/oqto-runner").write_text("#!/bin/true\n")
        pi = release / "immutable/pi-runtime/pi/pi"
        pi.parent.mkdir(parents=True)
        pi.write_text("#!/bin/sh\necho 1.1.0\n")
        pi.chmod(0o755)
        (release / "manifest.toml").write_text(
            f'[release]\ntarget = "full"\nversion = "{version}"\npi_version = "1.1.0"\n'
        )
        (releases / "current").symlink_to(release)
        self.host.exe_path = (release / "immutable/bin/oqto-runner").resolve()
        return releases

    def check_stage(self, profile: str = "team") -> Path:
        stage = self.root / "stage"
        stage.mkdir()
        (stage / "before.json").write_text(
            json.dumps(
                {
                    "profile": profile,
                    "principals": [self.principal.receipt()],
                    "health_url": "unused",
                    "health_budget": 1,
                }
            )
        )
        return stage

    def run_check(self, reply: dict, profile: str = "team") -> Path:
        tx.RELEASES = self.make_release()
        original = tx.health
        tx.health = lambda url: reply
        try:
            stage = self.check_stage(profile)
            tx.check(stage, self.host)
            return stage
        finally:
            tx.health = original
            tx.RELEASES = Path("/var/lib/oqto/releases")

    def test_check_proves_version_runner_executable_and_bundled_pi(self) -> None:
        stage = self.run_check({"status": "ok", "version": "0.5.9"}, profile="personal")
        receipt = json.loads((stage / "service-check.json").read_text())
        self.assertEqual(
            receipt["release"], {"version": "0.5.9", "pi_version": "1.1.0"}
        )
        self.assertEqual(receipt["runners"][0]["pid"], 4242)

    def test_check_rejects_stale_backend_version(self) -> None:
        with self.assertRaises(tx.TransactionError):
            self.run_check({"status": "ok", "version": "0.5.8"})

    def test_check_rejects_runner_on_previous_binary(self) -> None:
        original = tx.health
        tx.health = lambda url: {"status": "ok", "version": "0.5.9"}
        tx.RELEASES = self.make_release()
        try:
            self.host.exe_path = Path(
                "/var/lib/oqto/releases/old/immutable/bin/oqto-runner"
            )
            with self.assertRaises(tx.TransactionError):
                tx.check(self.check_stage(), self.host)
        finally:
            tx.health = original
            tx.RELEASES = Path("/var/lib/oqto/releases")

    def test_check_fails_when_personal_doctor_fails(self) -> None:
        self.host.doctor_exit = 1
        with self.assertRaises(tx.TransactionError):
            self.run_check({"status": "ok", "version": "0.5.9"}, profile="personal")

    def test_activate_and_backfill_refuse_non_root(self) -> None:
        if os.geteuid() == 0:
            self.skipTest("running as root")
        args = argparse.Namespace(receipt_root=self.receipts)
        with self.assertRaises(tx.TransactionError):
            tx.activate(args, self.host)
        with self.assertRaises(tx.TransactionError):
            tx.backfill(backfill_args(self.receipts), self.host)


if __name__ == "__main__":
    unittest.main()
