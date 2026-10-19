#!/bin/sh
# Resolve current exactly once: existing sessions never follow a later activation.
set -eu
self=$(/usr/bin/readlink -f -- "$0")
exec /usr/bin/python3 -I "${self%/*}/launch.py" "$@"
