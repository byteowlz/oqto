#!/usr/bin/env bash
# Pinned-source resolution: private working repo preferred, public mirror as the
# authority for rewritten public pins. Uses real git repositories, no network.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/dist/pi-ext-sources.sh
source "$root/scripts/dist/pi-ext-sources.sh"

scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT

make_repo() {
  local path="$1" message="$2"
  git init -q "$path"
  git -C "$path" -c user.name=fixture -c user.email=fixture@example.test \
    commit -q --allow-empty -m "$message"
  git -C "$path" rev-parse HEAD
}

private="$scratch/private"
public="$scratch/public"
private_head="$(make_repo "$private" private-only)"
public_head="$(make_repo "$public" published)"
urls="file://$private file://$public"

fail() { echo "FAIL: $*" >&2; exit 1; }

# 1. The private working repository is preferred when it has the ref.
first="$(pi_ext_resolve_source "$scratch/cache1" "$scratch/no-local" "$urls" "$private_head" fixture 2>"$scratch/log1")"
[[ "$(git -C "$first" rev-parse HEAD)" == "$private_head" ]] || fail "private ref not resolved"
grep -Fq "cloning fixture from file://$private" "$scratch/log1" || fail "did not prefer the private URL"
grep -Fq "resolved it from" "$scratch/log1" && fail "unexpected fallback for a private ref"

# 2. A public-only (rewritten) pin falls back to the mirror and says so.
second="$(pi_ext_resolve_source "$scratch/cache2" "$scratch/no-local" "$urls" "$public_head" fixture 2>"$scratch/log2")"
[[ "$(git -C "$second" rev-parse HEAD)" == "$public_head" ]] || fail "public pin not resolved"
grep -Fq "is not in file://$private; resolved it from file://$public" "$scratch/log2" \
  || { cat "$scratch/log2" >&2; fail "fallback was not reported"; }

# 3. Neither URL having the pin fails closed and names both.
if pi_ext_resolve_source "$scratch/cache3" "$scratch/no-local" "$urls" \
  0000000000000000000000000000000000000000 fixture >"$scratch/out3" 2>"$scratch/log3"; then
  fail "missing pin was accepted"
fi
grep -Fq "exists in none of: file://$private file://$public" "$scratch/log3" \
  || { cat "$scratch/log3" >&2; fail "failure did not name both sources"; }
grep -Fq "commit-map.txt" "$scratch/log3" || fail "failure did not point at the commit map"

# 4. A dirty sibling checkout is never used, and stays untouched.
sibling="$scratch/sibling"
make_repo "$sibling" unpinned >/dev/null
printf 'work in progress\n' > "$sibling/wip"
resolved="$(pi_ext_resolve_source "$scratch/cache4" "$sibling" "$urls" "$private_head" fixture 2>"$scratch/log4")"
[[ "$resolved" != "$sibling" ]] || fail "dirty sibling was reused"
grep -Fq "is not the clean pinned commit" "$scratch/log4" || fail "dirty sibling not reported"
grep -Fxq 'work in progress' "$sibling/wip" || fail "sibling was modified"

echo "dist extension source resolution: private-first, public fallback, fail-closed OK"
