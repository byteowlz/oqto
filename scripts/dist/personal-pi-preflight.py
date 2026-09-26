#!/usr/bin/env python3
"""Read-only, offline first-personal-install Pi readiness inventory.

This deliberately does not start Pi: even a model-list RPC can create auth.json.
No credential content is read, and no network/provider request is made.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path


def inspect(pi: Path, agent_dir: Path, route: str, model: str | None) -> dict:
    binary = pi.is_file() and os.access(pi, os.X_OK)
    # Presence is not proof of valid authentication; never open or print this file.
    credential_present = (agent_dir / "auth.json").exists()
    return {
        "binary_acquired": binary,
        "provider_route": route,
        "human_consent": "not_verified",
        "provider_auth": "not_verified",
        "credential_file_present": credential_present,
        "model_selection": "declared_not_verified" if model else "missing",
        "chat_readiness": "not_verified",
        "next_steps": (
            (["Acquire and verify the pinned Pi runtime"] if not binary else [])
            + (["Explicitly configure EAVS with a scoped key and verify reachability"]
               if route == "eavs" else [
                   "With user consent, use native Pi /login (Codex/ChatGPT where supported), "
                   "an API key, or a local model; do not start OAuth in preflight"
               ])
            + (["Select a model in Pi /model"] if not model else [
                "Verify the declared model in Pi's runner-side available catalog"
            ])
            + ["Prove a real streamed Pi RPC turn and reconnect before claiming chat readiness"]
        ),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pi", type=Path, required=True,
                        help="Path to already acquired Pi executable")
    parser.add_argument("--agent-dir", type=Path, required=True,
                        help="Pi agent directory; only auth.json existence is checked")
    parser.add_argument("--route", choices=("direct", "eavs"), required=True,
                        help="Explicit provider route; EAVS is never implicit")
    parser.add_argument("--model", help="Declared model selection (not validated against Pi)")
    args = parser.parse_args()
    print(json.dumps(inspect(args.pi, args.agent_dir, args.route, args.model), sort_keys=True))


if __name__ == "__main__":
    main()
