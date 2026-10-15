#!/usr/bin/env bash
# Deploy driver policy regressions: everything here runs as --dry-run or fails
# before any host contact, against a throwaway hosts file and git checkout.
# Assertions match the literal "$transaction" text of the printed remote script.
# shellcheck disable=SC2016
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEPLOY="$ROOT_DIR/scripts/deploy.sh"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

cat >"$tmpdir/hosts.toml" <<'EOF'
[[host]]
name = "test-local"
ssh = ""
local = true
mode = "single-user"
user = "test"
frontend = true
web_root = "/var/www/oqto"
EOF

# A negated grep never fails under set -e; refute must exit explicitly.
refute() {
  local label="$1" pattern="$2" text="$3"
  if grep -q -- "$pattern" <<<"$text"; then echo "FAIL: $label matched '$pattern'" >&2; exit 1; fi
}

expect_fail() {
  local label="$1" needle="$2"; shift 2
  if "$@" >"$tmpdir/out" 2>&1; then
    echo "FAIL: $label unexpectedly succeeded" >&2; cat "$tmpdir/out" >&2; exit 1
  fi
  grep -q -- "$needle" "$tmpdir/out" || { echo "FAIL: $label lacks '$needle'" >&2; cat "$tmpdir/out" >&2; exit 1; }
}

printf 'fixture\n' > "$tmpdir/oqto-0.0.1-x86_64-unknown-linux-gnu.tar.gz"
(cd "$tmpdir" && sha256sum oqto-0.0.1-x86_64-unknown-linux-gnu.tar.gz > oqto-0.0.1-x86_64-unknown-linux-gnu.tar.gz.sha256)
artifact=(--artifact "$tmpdir/oqto-0.0.1-x86_64-unknown-linux-gnu.tar.gz"
  --checksum "$tmpdir/oqto-0.0.1-x86_64-unknown-linux-gnu.tar.gz.sha256")
common=(--host test-local --config "$tmpdir/hosts.toml" --dry-run)

# Release vs development dependency policy.
release_output="$("$DEPLOY" "${common[@]}" "${artifact[@]}" --mode release 2>&1)"
grep -q 'dependency gate: eavs >=' <<<"$release_output"
refute 'release mode' 'installed version preserved' "$release_output"
dev_output="$("$DEPLOY" "${common[@]}" "${artifact[@]}" --mode dev 2>&1)"
grep -q 'dependency presence gate: eavs (installed version preserved)' <<<"$dev_output"
refute 'dev mode' 'dependency gate: eavs >=' "$dev_output"
expect_fail "unknown mode" "Invalid --mode 'unsupported'" "$DEPLOY" --mode unsupported --config "$tmpdir/hosts.toml"

# Activation goes through the verified bootstrap and one root transaction.
grep -q 'verified-setup-bootstrap.py' <<<"$release_output"
grep -q 'transaction=.*/deploy-transaction.py' <<<"$release_output"
grep -q 'python3 "$transaction" activate' <<<"$release_output"
grep -q -- '--profile personal --principal test' <<<"$release_output"
refute 'activation' 'migrate-oqto-log\|pkill\|mmry-jsonl' "$release_output"

# History is never part of activation; backfill is explicit and separate.
refute 'activation' 'python3 "$transaction" backfill' "$release_output"
backfill_output="$("$DEPLOY" "${common[@]}" --backfill-only --backfill-modes sync-identities,validate 2>&1)"
grep -q 'python3 "$transaction" backfill' <<<"$backfill_output"
grep -qE -- '--modes sync-identities\\?,validate' <<<"$backfill_output"  # %q escapes the comma
refute 'backfill-only' 'activate\|dist-stage\|verified-setup-bootstrap' "$backfill_output"
expect_fail "destructive backfill mode" "--backfill-modes must list only" \
  "$DEPLOY" "${common[@]}" --backfill-only --backfill-modes rebuild-from-jsonl
expect_fail "conflicting backfill flags" "mutually exclusive" \
  "$DEPLOY" "${common[@]}" --backfill --backfill-only

# Overrides are explicit and forwarded verbatim.
override_output="$("$DEPLOY" "${common[@]}" "${artifact[@]}" --allow-downgrade --allow-dirty-source 2>&1)"
grep -q -- '--allow-downgrade' <<<"$override_output"
grep -q -- '--allow-dirty-source' <<<"$override_output"
refute 'default policy' '--allow-downgrade' "$release_output"

# Artifact inputs and removed legacy flags fail closed.
printf 'not a release\n' > "$tmpdir/unverified.tar.gz"
expect_fail "artifact without checksum" "checksum" "$DEPLOY" "${common[@]}" --artifact "$tmpdir/unverified.tar.gz"
for legacy in --skip-build --allow-legacy-path --prepare-only --resume --skip-frontend --remote-build --trace-streams; do
  expect_fail "legacy flag $legacy" "Unknown option" "$DEPLOY" "${common[@]}" "$legacy"
done

# Building from a dirty or untracked worktree is refused before host contact.
checkout="$tmpdir/checkout"
mkdir -p "$checkout/scripts/dist" "$checkout/deploy" "$checkout/backend"
cp "$DEPLOY" "$checkout/scripts/deploy.sh"
cp "$ROOT_DIR/scripts/dist/verify-immutable-migrations.py" "$ROOT_DIR/scripts/dist/deploy-sudo.sh" "$checkout/scripts/dist/"
cp "$ROOT_DIR/dependencies.toml" "$checkout/"
printf '[workspace.package]\nversion = "0.0.1"\n' > "$checkout/backend/Cargo.toml"
cp "$tmpdir/hosts.toml" "$checkout/deploy/hosts.toml"
migration=backend/crates/oqto/migrations/20260901001_oqto_apps_v0.sql
mkdir -p "$checkout/$(dirname "$migration")"
cp "$ROOT_DIR/$migration" "$checkout/$migration"
git -C "$checkout" init -q
git -C "$checkout" -c user.email=t@example.invalid -c user.name=t add -A
git -C "$checkout" -c user.email=t@example.invalid -c user.name=t commit -qm fixture
printf 'scratch\n' > "$checkout/untracked.txt"
expect_fail "dirty build" "uncommitted or untracked" "$checkout/scripts/deploy.sh" --host test-local --dry-run

# The deploy build must stage everything the canonical release build stages;
# a step missing here only shows up on a clean checkout (dist/ is untracked).
python3 - "$DEPLOY" "$ROOT_DIR/.github/workflows/release.yml" <<'PY'
import re, sys
deploy, release = (open(path).read() for path in sys.argv[1:])
body = deploy.split("\nbuild_artifact() {\n", 1)[1].split("\n}\n", 1)[0]
script_for = {
    "dist-sync": "sync.sh",
    "dist-stage-pi-runtime": "stage-pi-runtime.sh",
    "dist-stage-binaries": "stage-binaries.sh",
    "dist-stage-frontend": "stage-frontend.sh",
    "dist-package": "package.sh",
}
deploy_steps = [step for step in re.findall(r"^\s+(?:[A-Z_]+=\S+\s+)*just (dist-[a-z-]+)", body, re.M) if step in script_for]
release_steps = [name for name in re.findall(r"scripts/dist/([a-z-]+\.sh)", release) if name in script_for.values()]
mapped = [script_for[step] for step in deploy_steps]
assert mapped == release_steps, f"deploy build {mapped} != release build {release_steps}"
PY

help_output="$("$DEPLOY" --help)"
grep -q -- '--backfill-only' <<<"$help_output"
grep -q -- '--allow-downgrade' <<<"$help_output"

echo "deploy mode tests passed"
