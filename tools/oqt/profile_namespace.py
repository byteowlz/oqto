"""Catalog identity: canonical workspace, runtime mode and explicit owner profile.

Only path metadata is inspected; owner credentials/settings are never loaded.
"""
import hashlib
import json
from pathlib import Path


def workspace_profile_key(workspace, mode, owner=None):
    if mode not in ("coding", "demo"):
        raise ValueError("Unsupported runtime mode")
    workspace = Path(workspace).expanduser().resolve(strict=True)
    if not workspace.is_dir():
        raise ValueError("Work directory must exist")
    canonical_owner = None
    if mode == "coding":
        if owner is None or not str(owner):
            raise ValueError("Coding catalog requires an explicit owner profile")
        canonical_owner = Path(owner).expanduser().resolve(strict=True)
        if not canonical_owner.is_dir():
            raise ValueError("Owner profile must be a directory")
    value = [str(workspace), mode, str(canonical_owner) if canonical_owner else None]
    return hashlib.sha256(json.dumps(value, separators=(",", ":")).encode()).hexdigest()[:12]
