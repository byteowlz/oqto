#!/usr/bin/env python3
"""Append one administrator-owned runner registration without rewriting TOML.

Existing targets and unknown settings are preserved byte-for-byte. Conflicting
IDs fail rather than changing routing/security policy. No private key material
is accepted: the endpoint contains local credential file paths only.
"""
import argparse
import json
import os
import re
from pathlib import Path
import tempfile
import time
import tomllib


def register(path: Path, target: dict, dry_run: bool = False) -> str:
    path = path.resolve(strict=True)
    original = path.read_bytes()
    config = tomllib.loads(original.decode())
    current = config.get("backend", {}).get("runner", {}).get("targets", [])
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", target["id"]):
        raise ValueError("invalid target ID")
    if not target["label"].strip() or len(target["label"].encode()) > 80 or any(ord(c) < 32 or ord(c) == 127 for c in target["label"]):
        raise ValueError("invalid target label")
    if not isinstance(current, list):
        raise ValueError("existing targets must be an array")
    for existing in current:
        if existing.get("id") == target["id"]:
            if existing == target:
                return "unchanged"
            raise ValueError("target ID already exists with different settings; refusing replacement")
    if len(current) >= 32:
        raise ValueError("at most 32 runner targets may be configured")
    endpoint = target["endpoint"]
    if endpoint.get("transport") != "tcp_tls" or set(endpoint) != {"transport", "address", "server_name", "ca", "certificate", "key"}:
        raise ValueError("expected a tcp_tls endpoint with credential file paths")
    if not target["account_ids"] or not all(isinstance(a, str) and a.strip() for a in target["account_ids"]) or not all(Path(endpoint[k]).is_absolute() for k in ("ca", "certificate", "key")):
        raise ValueError("explicit Account grants and absolute credential paths are required")
    q = json.dumps
    addition = "\n\n# Explicit remote inventory; does not rebind any Workspace or Session.\n[[backend.runner.targets]]\n"
    for key in ("id", "label", "account_ids"):
        addition += f"{key} = {q(target[key])}\n"
    addition += "[backend.runner.targets.endpoint]\n"
    for key, value in endpoint.items():
        addition += f"{key} = {q(value)}\n"
    updated = original + addition.encode()
    parsed = tomllib.loads(updated.decode())
    if parsed["backend"]["runner"]["targets"][-1] != target:
        raise ValueError("registration round-trip failed")
    if dry_run:
        return addition
    backup = path.with_name(path.name + f".before-runner-target-{time.time_ns()}")
    with os.fdopen(os.open(backup, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "wb") as file:
        file.write(original)
    descriptor, temporary = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as file:
            os.fchmod(file.fileno(), path.stat().st_mode & 0o777)
            file.write(updated)
            file.flush()
            os.fsync(file.fileno())
        if path.read_bytes() != original:
            raise ValueError("config changed concurrently; refusing overwrite")
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return f"registered {target['id']}; backup: {backup}"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--id", required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--account-id", action="append", required=True)
    parser.add_argument("--endpoint", type=Path, required=True)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    print(register(args.config, {"id": args.id, "label": args.label, "account_ids": args.account_id,
                                "endpoint": json.loads(args.endpoint.read_text())}, args.dry_run))
