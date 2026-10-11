#!/usr/bin/env bash
# Run ONLY the independent upstream service regression, with bounded host resources.
set -euo pipefail
readonly pin=cdaf6c227d4718a72b59ccd02152c9614df44cfc
if [[ $# -ne 1 ]]; then
  printf 'Usage: %s /tmp/oqto-pi-service-spike.XXXXXX/source\n' "$0" >&2
  exit 2
fi
source_dir=$(realpath "$1")
case "$source_dir" in
  /tmp/oqto-pi-service-spike.*/source) ;;
  *) printf 'Refusing a non-disposable source checkout: %s\n' "$source_dir" >&2; exit 2 ;;
esac
[[ $(git -C "$source_dir" rev-parse HEAD) == "$pin" ]] || {
  printf 'Wrong upstream pin; expected %s\n' "$pin" >&2; exit 2;
}
git -C "$source_dir" diff --quiet && git -C "$source_dir" diff --cached --quiet || {
  printf 'Refusing modified tracked upstream source.\n' >&2; exit 2;
}
[[ -f "$source_dir/node_modules/vitest/vitest.mjs" && -f "$source_dir/packages/durable/dist/index.js" ]] || {
  printf 'First prepare dependencies, hydrate model data and build:offline in this disposable checkout.\n' >&2
  exit 2
}
for command in taskset nice timeout python3; do command -v "$command" >/dev/null; done
readonly here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
readonly profile_home=$(dirname -- "$source_dir")/home
mkdir -p "$profile_home"
readonly cpu=$(python3 -c 'import os; print(max(os.sched_getaffinity(0)))')
test_file=$(mktemp "$source_dir/packages/coding-agent/test/oqto-resource-bounded-XXXXXX.test.ts")
trap 'rm -f -- "$test_file"' EXIT
cp -- "$here/multiclient.test.ts" "$test_file"
cd "$source_dir"
printf 'Source=%s CPU=%s nice=19 workers=1 heap=512MiB; one explicit test file only\n' "$pin" "$cpu"
# CPU affinity and priority propagate to spawned server/session workers.
# The explicit test path is essential: --config alone does NOT constrain Vitest's root.
timeout --signal=TERM --kill-after=10s 90s \
  taskset -c "$cpu" nice -n 19 \
  env -i PATH="$PATH" HOME="$profile_home" USER="${USER:-spike}" LOGNAME="${LOGNAME:-spike}" \
  NODE_OPTIONS=--max-old-space-size=512 PI_OFFLINE=1 PI_EXPERIMENTAL=1 \
  node node_modules/vitest/vitest.mjs run \
  --config packages/coding-agent/vitest.config.ts "$test_file" \
  --maxWorkers=1 --no-file-parallelism --reporter=verbose --silent=false
