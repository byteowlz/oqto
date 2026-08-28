#!/usr/bin/env python3
"""Preserve-first merge of top-level deny_read paths into sandbox.toml."""

from __future__ import annotations

import os
from pathlib import Path
import re
import sys
import tempfile
import tomllib


def _array_end(text: str, start: int) -> int:
    depth = 0
    quote: str | None = None
    escaped = False
    comment = False
    for index in range(start, len(text)):
        char = text[index]
        if comment:
            if char == "\n":
                comment = False
            continue
        if quote:
            if escaped:
                escaped = False
            elif char == "\\" and quote == '"':
                escaped = True
            elif char == quote:
                quote = None
            continue
        if char == "#":
            comment = True
        elif char in {'"', "'"}:
            quote = char
        elif char == "[":
            depth += 1
        elif char == "]":
            depth -= 1
            if depth == 0:
                return index
    raise ValueError("unterminated top-level deny_read array")


def merge_deny_read(text: str, required_paths: list[str]) -> str:
    parsed = tomllib.loads(text)
    current = parsed.get("deny_read", [])
    if not isinstance(current, list) or not all(isinstance(item, str) for item in current):
        raise ValueError("top-level deny_read must be an array of strings")
    missing = [path for path in required_paths if path not in current]
    if not missing:
        return text

    match = re.search(r"(?m)^deny_read\s*=\s*\[", text)
    rendered = [f'"{path}"' for path in missing]
    if match:
        opening = text.find("[", match.start())
        closing = _array_end(text, opening)
        body = text[opening + 1 : closing]
        if "\n" in body:
            before = body.rstrip()
            suffix = body[len(before) :]
            comma = "" if not before or before.rstrip().endswith(",") else ","
            insertion = comma + "\n" + "".join(f"    {item},\n" for item in rendered)
            updated = text[: opening + 1] + before + insertion + suffix + text[closing:]
        else:
            separator = ", " if body.strip() else ""
            updated = text[:closing] + separator + ", ".join(rendered) + text[closing:]
    else:
        table = re.search(r"(?m)^\s*\[[^[]", text)
        index = table.start() if table else len(text)
        prefix = "# oqto-setup: runner control sockets are never workspace-readable.\n"
        prefix += "deny_read = [" + ", ".join(rendered) + "]\n\n"
        updated = text[:index] + prefix + text[index:]

    tomllib.loads(updated)
    return updated


def main() -> int:
    if len(sys.argv) < 3:
        print(f"usage: {sys.argv[0]} SANDBOX_TOML PATH [PATH ...]", file=sys.stderr)
        return 2
    config = Path(sys.argv[1])
    original = config.read_text()
    updated = merge_deny_read(original, sys.argv[2:])
    if updated == original:
        return 0

    stat = config.stat()
    fd, temporary = tempfile.mkstemp(prefix=f".{config.name}.", dir=config.parent)
    try:
        with os.fdopen(fd, "w") as handle:
            handle.write(updated)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, stat.st_mode)
        os.chown(temporary, stat.st_uid, stat.st_gid)
        os.replace(temporary, config)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
