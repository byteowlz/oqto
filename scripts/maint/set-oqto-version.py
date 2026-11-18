"""Set the [oqto] version in dependencies.toml (portable: no GNU-only sed)."""
import pathlib
import re
import sys


def main() -> int:
    path, version = pathlib.Path(sys.argv[1]), sys.argv[2]
    text = path.read_text()
    table = re.compile(r"(?ms)^\[oqto\]$.*?(?=^\[|\Z)")
    block = table.search(text)
    if block is None:
        print("dependencies.toml has no [oqto] table", file=sys.stderr)
        return 1
    updated, count = re.subn(
        r'(?m)^version = "[^"]*"', f'version = "{version}"', block.group(0), count=1
    )
    if count != 1:
        print("[oqto] has no version line", file=sys.stderr)
        return 1
    path.write_text(text[: block.start()] + updated + text[block.end() :])
    return 0


if __name__ == "__main__":
    sys.exit(main())
