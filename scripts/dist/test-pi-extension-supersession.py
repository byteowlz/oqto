#!/usr/bin/env python3
"""Real Pi autoload, effective closure and mock-provider migration qualification.

Explicit binaries/assets only; all HOME/XDG/settings/JSONL and HTTP are disposable.
No production credentials, services, sessions, provider endpoints or profiles.
"""

import argparse
from http.server import ThreadingHTTPServer
import importlib.util
import io
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import tempfile
import threading

HERE = Path(__file__).resolve().parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


WIRE = load("wire_fixture", HERE / "test-pi-rpc-qualification.py")
MANAGED = load("managed", HERE / "managed-pi-extensions.py")


def asset_fingerprint(source, names):
    return {
        name: {
            str(path.relative_to(source / name)): (
                "link:" + os.readlink(path)
                if path.is_symlink()
                else MANAGED.digest(path)
            )
            for path in sorted((source / name).rglob("*"))
            if path.is_symlink() or path.is_file()
        }
        for name in names
    }


def rpc_env(home):
    return {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_CACHE_HOME": str(home / ".cache"),
        "XDG_DATA_HOME": str(home / ".local/share"),
        "PI_CODING_AGENT_DIR": str(home / ".pi/agent"),
        "PI_OFFLINE": "1",
    }


class Handler(WIRE.FixtureHandler):
    def do_POST(self):
        raw = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        body = json.loads(raw)
        self.server.tool_sets.append(
            [tool["function"]["name"] for tool in body.get("tools", [])]
        )
        self.rfile = io.BytesIO(raw)
        super().do_POST()


class EffectiveRpc(WIRE.Rpc):
    def __init__(self, binary, home, cwd, stderr, session=None):
        # Unlike the upstream protocol-only fixture, do not disable discovery,
        # packages, tools or context. Approve only this disposable project.
        args = [
            str(binary),
            "--mode",
            "rpc",
            "--offline",
            "--approve",
            "--provider",
            "oqto-qualification",
            "--model",
            "fixture",
        ]
        args += (
            ["--session", str(session)]
            if session
            else ["--session-dir", str(home / "sessions")]
        )
        self.proc = subprocess.Popen(
            args,
            cwd=cwd,
            env=rpc_env(home),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=stderr,
        )
        self.incoming, self.records, self.counter = queue.Queue(), [], 0
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()


def fixture_tool(name):
    return f"""export default function(pi) {{ pi.registerTool({{
 name: {json.dumps(name)}, label: {json.dumps(name)}, description: "Synthetic closure fixture",
 parameters: {{type:"object",properties:{{}}}},
 async execute() {{return {{content:[{{type:"text",text:"fixture"}}],details:{{}}}};}}
 }}); }}\n"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--legacy-source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    policy = json.loads((HERE / "pi-extension-supersessions.json").read_text())
    names = [
        line.strip()
        for line in (HERE / "pi-default-extensions.txt").read_text().splitlines()
        if line.strip() and not line.startswith("#")
    ]
    assert (
        MANAGED.index_digest(args.legacy_source)
        == "30dc9d04507ad31b72938770cf6ffbf955a92adf502fff67a620975d80f1c3cb"
    )
    args.output.mkdir(parents=True, exist_ok=False)
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.tool_sets, server.requests, server.hold = [], 0, False
    server.started, server.release = threading.Event(), threading.Event()
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix="oqto-extension-supersession-") as temp:
            root = Path(temp)
            home, cwd = root / "home", root / "project"
            agent = home / ".pi/agent"
            agent.mkdir(parents=True)
            cwd.mkdir()
            MANAGED.reconcile(home, args.source, names, policy)
            extensions = agent / "extensions"
            shutil.copytree(args.legacy_source, extensions / "pi-oqto-todos")
            # A genuine unknown personal extension, a trusted project extension,
            # and a package discovered through effective personal settings.
            for base, name in [
                (extensions / "user-custom", "UserFixture"),
                (cwd / ".pi/extensions/project-custom", "ProjectFixture"),
                (root / "package/extensions/package-custom", "PackageFixture"),
            ]:
                base.mkdir(parents=True)
                (base / "index.ts").write_text(fixture_tool(name))
            (root / "package/package.json").write_text(
                json.dumps(
                    {
                        "name": "oqto-fixture-package",
                        "pi": {"extensions": ["extensions/package-custom/index.ts"]},
                    }
                )
            )
            settings = {
                "packages": [str(root / "package")],
                "compaction": {"enabled": False},
                "defaultTools": [
                    "read",
                    "Todo",
                    "UserFixture",
                    "ProjectFixture",
                    "PackageFixture",
                ],
            }
            (agent / "settings.json").write_text(json.dumps(settings))
            (agent / "models.json").write_text(
                json.dumps(
                    {
                        "providers": {
                            "oqto-qualification": {
                                "baseUrl": f"http://127.0.0.1:{server.server_port}/v1",
                                "api": "openai-completions",
                                "apiKey": "synthetic-no-secret",
                                "models": [
                                    {
                                        "id": "fixture",
                                        "name": "Fixture",
                                        "reasoning": True,
                                        "contextWindow": 32768,
                                        "maxTokens": 256,
                                        "input": ["text"],
                                        "cost": {
                                            "input": 0,
                                            "output": 0,
                                            "cacheRead": 0,
                                            "cacheWrite": 0,
                                        },
                                    }
                                ],
                            }
                        }
                    }
                )
            )
            task_data = cwd / ".pi/todos/retained-old-native/todos.json"
            task_data.parent.mkdir(parents=True)
            task_bytes = json.dumps(
                {
                    "todos": [
                        {
                            "id": "existing-task",
                            "content": "synthetic retained task",
                            "status": "pending",
                            "priority": "high",
                        }
                    ],
                    "updated_at": "2026-10-20T00:00:00Z",
                }
            )
            task_data.write_text(task_bytes)
            binary_digest = MANAGED.digest(binary)
            binary_version = subprocess.run(
                [str(binary), "--version"],
                cwd=cwd,
                env=rpc_env(home),
                capture_output=True,
                text=True,
                timeout=10,
                check=True,
            ).stdout.strip()
            source_fingerprint = asset_fingerprint(args.source, names)
            original_settings = (agent / "settings.json").read_bytes()
            with (args.output / "baseline.stderr").open("wb") as stderr:
                rpc = EffectiveRpc(binary, home, cwd, stderr)
                try:
                    assert rpc.proc.wait(timeout=15) != 0, (
                        "duplicate autoload unexpectedly accepted"
                    )
                finally:
                    rpc.close()
            assert (
                b'Tool "Todo" conflicts'
                in (args.output / "baseline.stderr").read_bytes()
            )
            first = MANAGED.reconcile(home, args.source, names, policy)
            assert "pi-oqto-todos" in first["archived"]
            assert not MANAGED.reconcile(home, args.source, names, policy)["changed"]
            assert task_data.read_text() == task_bytes
            assert (agent / "settings.json").read_bytes() == original_settings
            with (args.output / "migrated.stderr").open("wb") as stderr:
                rpc = EffectiveRpc(binary, home, cwd, stderr)
                try:
                    state = rpc.command("get_state")
                    rpc.command(
                        "set_model", provider="oqto-qualification", modelId="fixture"
                    )
                    rpc.command("set_thinking_level", level="low")
                    selected = rpc.command("get_state")
                    assert selected["thinkingLevel"] == "low"
                    assert selected["model"]["provider"] == "oqto-qualification"
                    assert selected["model"]["id"] == "fixture"
                    rpc.command(
                        "prompt",
                        message="Synthetic migration fixture; answer without tools.",
                    )
                    rpc.settled()
                    state = rpc.command("get_state")
                    native_id, native_file = (
                        state["sessionId"],
                        Path(state["sessionFile"]),
                    )
                    messages = rpc.command("get_messages")["messages"]
                    assert any(m.get("role") == "assistant" for m in messages)
                    assert native_file.is_file()
                finally:
                    rpc.close()
            (args.output / "provider-tool-sets.json").write_text(
                json.dumps(server.tool_sets, indent=2)
            )
            for tools in server.tool_sets:
                assert len(tools) == len(set(tools)), tools
            # Background naming/summarization legitimately uses no tools. The
            # actual agent request must expose the entire effective fixture set.
            required = {
                "read",
                "Todo",
                "rename_session",
                "Selection",
                "self_reflection",
                "HistorySearch",
                "HistoryRead",
                "HistoryGrep",
                "HistoryBranches",
                "UserFixture",
                "ProjectFixture",
                "PackageFixture",
            }
            assert any(required.issubset(tools) for tools in server.tool_sets), (
                server.tool_sets
            )
            assert server.requests > 0
            with (args.output / "restored.stderr").open("wb") as stderr:
                rpc = EffectiveRpc(binary, home, cwd, stderr, native_file)
                try:
                    state = rpc.command("get_state")
                    assert state["sessionId"] == native_id
                    assert any(
                        m.get("role") == "assistant"
                        for m in rpc.command("get_messages")["messages"]
                    )
                finally:
                    rpc.close()
            # Unknown duplicate project/package code is never auto-retired.
            unknown = cwd / ".pi/extensions/unknown-conflict"
            unknown.mkdir()
            (unknown / "index.ts").write_text(fixture_tool("Todo"))
            assert not MANAGED.reconcile(home, args.source, names, policy)["changed"]
            with (args.output / "unknown-conflict.stderr").open("wb") as stderr:
                rpc = EffectiveRpc(binary, home, cwd, stderr)
                try:
                    assert rpc.proc.wait(timeout=15) != 0
                finally:
                    rpc.close()
            assert unknown.is_dir()
            assert task_data.read_text() == task_bytes
            assert (
                b'Tool "Todo" conflicts'
                in (args.output / "unknown-conflict.stderr").read_bytes()
            )
            receipt = {
                "version": 1,
                "binary": str(binary),
                "binary_version": binary_version,
                "binary_sha256": binary_digest,
                "source_asset_sha256": source_fingerprint,
                "baseline_duplicate_rejected": True,
                "migration_idempotent": True,
                "unknown_personal_project_package_extensions_preserved": True,
                "unique_tools_in_real_provider_request": True,
                "required_tools_in_provider_request": sorted(required),
                "existing_todo_storage_preserved": True,
                "serialized_model_thinking": True,
                "mock_provider_turn": True,
                "same_native_id_cold_restore": True,
                "unknown_duplicate_fails_closed": True,
                "production_profile_or_credentials_used": False,
            }
            assert MANAGED.digest(binary) == binary_digest, (
                "runtime changed during qualification"
            )
            assert asset_fingerprint(args.source, names) == source_fingerprint, (
                "source changed during qualification"
            )
            (args.output / "receipt.json").write_text(
                json.dumps(receipt, indent=2) + "\n"
            )
            print(json.dumps(receipt))
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
