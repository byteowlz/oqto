"""Select committed, staged, unstaged and untracked Rust source for guardrails."""
import argparse
import os
from pathlib import Path
import subprocess
import sys


def changed_paths(root: Path, base: str) -> list[str]:
    commands = [
        ['diff', '--name-only', '-z', '--diff-filter=ACMR', f'{base}...HEAD', '--', '*.rs'],
        ['diff', '--name-only', '-z', '--diff-filter=ACMR', 'HEAD', '--', '*.rs'],
        ['ls-files', '--others', '--exclude-standard', '-z', '--', '*.rs'],
    ]
    candidates = set()
    for command in commands:
        data = subprocess.check_output(['git', '-C', str(root), *command])
        candidates.update(os.fsdecode(name) for name in data.split(b'\0') if name)
    result = []
    for name in sorted(candidates):
        path = root / name
        if 'target' in Path(name).parts:
            continue
        if path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
            raise ValueError(f'Rust guardrail source must stay inside the repository: {name!r}')
        if path.is_file():
            result.append(name)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', required=True)
    args = parser.parse_args()
    for name in changed_paths(Path(__file__).resolve().parents[2], args.base):
        sys.stdout.buffer.write(os.fsencode(name) + b'\0')
