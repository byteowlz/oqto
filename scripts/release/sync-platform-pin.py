#!/usr/bin/env python3
"""Preserve manifest content while syncing its own-platform pin after just bump."""
import argparse
from pathlib import Path
import re


def synchronize(text: str, version: str) -> str:
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("version must be numeric major.minor.patch")
    sections = list(re.finditer(r"(?m)^\[oqto\]\s*$", text))
    if len(sections) != 1:
        raise ValueError("expected exactly one [oqto] section")
    start = sections[0].end()
    following = re.search(r"(?m)^\[", text[start:])
    end = start + following.start() if following else len(text)
    block = text[start:end]
    pattern = r'(?m)^(version\s*=\s*")[^"]*("[^\n]*)$'
    if len(list(re.finditer(pattern, block))) != 1:
        raise ValueError("expected exactly one platform version pin")
    block = re.sub(pattern, lambda match: match[1] + version + match[2], block)
    return text[:start] + block + text[end:]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument("--manifest", type=Path, default=Path(__file__).resolve().parents[2] / "dependencies.toml")
    args = parser.parse_args()
    original = args.manifest.read_text()
    updated = synchronize(original, args.version)
    if updated != original:
        args.manifest.write_text(updated)


if __name__ == "__main__":
    main()
