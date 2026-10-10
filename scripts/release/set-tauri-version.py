#!/usr/bin/env python3
"""Set tauri.conf.json's top-level version without re-serializing the file."""

import json
import re
import sys
from pathlib import Path


def set_version(text: str, version: str) -> str:
    # The top-level key is the first "version" at one indentation level.
    pattern = re.compile(r'^(\s*"version"\s*:\s*")[^"]*(")', re.M)
    updated, count = pattern.subn(rf"\g<1>{version}\g<2>", text, count=1)
    if count != 1 or json.loads(updated).get("version") != version:
        raise ValueError("tauri.conf.json has no top-level version to update")
    return updated


def main() -> int:
    path, version = Path(sys.argv[1]), sys.argv[2]
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        print(f"error: invalid version {version!r}", file=sys.stderr)
        return 1
    try:
        path.write_text(set_version(path.read_text(), version))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
