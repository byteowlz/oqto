#!/bin/sh
# The oqt executable. Resolves the activated runtime exactly once, then runs it.
# Nothing runs between this shell and node: no Python, no argument parser, no
# hashing of the install (ADR-0072). `oqt verify` is the explicit full check.
set -eu
self=$(/usr/bin/readlink -f -- "$0")
runtime=${self%/*}
artifact=${runtime##*/}
state=${XDG_STATE_HOME:-$HOME/.local/state}
# Node reads this before our code runs, and creates it with default modes — so it
# lives outside the private runtime state tree we validate below.
NODE_COMPILE_CACHE=${NODE_COMPILE_CACHE:-$state/oqt/cache/$artifact}
OQT_CURRENT_RUNTIME=$runtime
export NODE_COMPILE_CACHE OQT_CURRENT_RUNTIME
source=$runtime/pi-source/packages/coding-agent/src/experimental
if [ "${1-}" = verify ]; then
  shift
  exec /usr/bin/python3 -I "$runtime/verifier.py" verify --runtime "$runtime" "$@"
fi
exec "$runtime/bin/node" --import "$source/source-resolver.ts" "$source/installed-entry.ts" "$@"
