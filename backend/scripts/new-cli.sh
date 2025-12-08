#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: new-cli.sh <name> [--path DIR]

Create a new CLI project by cloning the current template into DIR (defaults to <name>).

Options:
  -h, --help      Show this message
      --path DIR  Destination directory for the new project
USAGE
}

die() {
  echo "new-cli.sh: $*" >&2
  exit 1
}

NAME=""
DEST=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --path)
      shift
      [[ $# -gt 0 ]] || die "--path requires an argument"
      DEST="$1"
      ;;
    -*)
      die "unknown option: $1"
      ;;
    *)
      if [[ -z "$NAME" ]]; then
        NAME="$1"
      else
        die "unexpected argument: $1"
      fi
      ;;
  esac
  shift
done

[[ -n "$NAME" ]] || die "project name is required"

if [[ ! "$NAME" =~ ^[a-zA-Z][a-zA-Z0-9_-]*$ ]]; then
  die "project name must start with a letter and contain only letters, numbers, '_' or '-'"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEMPLATE_ROOT="$(dirname "$SCRIPT_DIR")"

if [[ -z "$DEST" ]]; then
  DEST="$(dirname "$TEMPLATE_ROOT")/$NAME"
fi

case "$DEST" in
  /*) ;; # absolute path
  *) DEST="$PWD/$DEST" ;;
esac

if [[ -e "$DEST" ]]; then
  die "destination already exists: $DEST"
fi

mkdir -p "$(dirname "$DEST")"

python3 - "$TEMPLATE_ROOT" "$DEST" <<'PY'
import pathlib
import shutil
import sys

root = pathlib.Path(sys.argv[1])
dest = pathlib.Path(sys.argv[2])

def ignore(directory, contents):
    ignored = {'.git', 'target', '.DS_Store'}
    return ignored.intersection(contents)

shutil.copytree(root, dest, ignore=ignore)

for path in dest.rglob('new-cli.sh'):
    if path.is_file():
        path.chmod(path.stat().st_mode | 0o111)
PY

python3 - "$NAME" "$DEST" <<'PY'
import pathlib
import sys

name = sys.argv[1]
dest = pathlib.Path(sys.argv[2])

def replace(path: pathlib.Path):
    text = path.read_text()
    text = text.replace("rust-cli", name)
    text = text.replace("RUST_CLI", name.upper().replace("-", "_"))
    path.write_text(text)

files = [
    dest / "Cargo.toml",
    dest / "Cargo.lock",
    dest / "README.md",
    dest / "examples" / "config.toml",
]

for file in files:
    if file.exists():
        replace(file)
PY

echo "Created CLI project at $DEST"
