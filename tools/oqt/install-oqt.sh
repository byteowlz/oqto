#!/usr/bin/env bash
# Explicit developer-only source packaging; never the Oqto release installer.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source=${OQT_PI_SOURCE:-$HOME/.local/share/oqt/pi-source}
data=${XDG_DATA_HOME:-$HOME/.local/share}/oqt
activate=1
while (($#)); do
 case "$1" in
  --stage-only) activate=0; shift;;
  --data-root) data=${2:?--data-root requires a directory}; shift 2;;
  *) echo "Usage: install-oqt.sh [--stage-only] [--data-root ABSOLUTE_DIR]" >&2; exit 2;;
 esac
done
[[ "$data" == /* ]] || { echo 'Runtime data root must be absolute' >&2; exit 2; }
cpu=$(python3 -c 'import os; print(min(os.sched_getaffinity(0)))')
export CARGO_BUILD_JOBS=1
fixture=$(mktemp -d /var/tmp/oqt-package.XXXXXX)
trap 'rm -rf -- "$fixture"' EXIT
mkdir -m 700 "$fixture/owner" "$fixture/workspace"
cd "$root"
timeout --kill-after=5s 600s taskset -c "$cpu" nice -n 19 python3 tools/oqt/build-assets.py "$source" >/dev/null
timeout --kill-after=5s 90s taskset -c "$cpu" nice -n 19 python3 tools/oqt/prepare.py "$source" "$fixture/workspace" coding "$fixture/owner" > "$fixture/prepared.json"
args=(install --prepared-json "$fixture/prepared.json" --launcher tools/oqt/installed-entry.sh --node "$(command -v node)" --data-root "$data"
 --runtime-file launch.py=tools/oqt/installed-launch.py
 --runtime-file installed-state.py=tools/oqt/installed-state.py
 --runtime-file profile_namespace.py=tools/oqt/profile_namespace.py
 --runtime-file preparation_lock.py=tools/oqt/preparation_lock.py)
if ((activate)); then
 args+=(--activate --launcher-destination "$HOME/.local/bin/oqt")
fi
timeout --kill-after=5s 600s taskset -c "$cpu" nice -n 19 python3 tools/oqt/install-runtime.py "${args[@]}"
