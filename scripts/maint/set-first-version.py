"""Replace the first `version = "..."` line (portable: BSD sed has no address 0)."""
import pathlib
import re
import sys


def main() -> int:
    path, version = pathlib.Path(sys.argv[1]), sys.argv[2]
    updated, count = re.subn(
        r'(?m)^version = "[^"]*"', f'version = "{version}"', path.read_text(), count=1
    )
    if count != 1:
        print(f"no version line in {path}", file=sys.stderr)
        return 1
    path.write_text(updated)
    return 0


if __name__ == "__main__":
    sys.exit(main())
