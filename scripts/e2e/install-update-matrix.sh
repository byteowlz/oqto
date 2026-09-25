#!/usr/bin/env bash
set -euo pipefail

# Artifact-based install/update validation for oqto-vemr.8.
# Plan and preflight are read-only. Execute is intentionally VM-only and
# requires an operator snapshot reference; it never builds from source.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
MODE=plan
PROFILE_SET="personal team"
SCENARIO=""
SNAPSHOT_ID=""
ARTIFACT=""
CHECKSUM=""
ACTIVE_RELEASE=/var/lib/oqto/releases/current

usage() {
  cat <<'EOF'
Usage: scripts/e2e/install-update-matrix.sh [options]

Options:
  --preflight               Validate an artifact and target without changing anything
  --execute                 Install on a SNAPSHOTTED, isolated test VM (never source-build)
  --scenario fresh|upgrade  Required for preflight/execute
  --snapshot-id REF         Operator-provided VM snapshot reference (not independently verified)
  --profiles "personal"     Exactly one profile for preflight/execute; plan may show both
  --artifact FILE           Target-matched release bundle containing bin/oqto-setup
  --checksum FILE           Single-artifact SHA-256 line for that exact bundle
  -h, --help                Show help

Examples:
  scripts/e2e/install-update-matrix.sh
  scripts/e2e/install-update-matrix.sh --preflight --scenario upgrade \
    --snapshot-id before-upgrade --profiles personal \
    --artifact oqto-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz --checksum artifact.sha256

The VM must be snapshotted separately. Do not run --execute on a working host.
See docs/agents/install-matrix-safety.md.
EOF
}

fail() { printf 'error: %s\n' "$*" >&2; exit 1; }

show_cmd() {
  printf '[matrix]'
  printf ' %q' "$@"
  printf '\n'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --preflight|--execute)
      [[ "$MODE" == plan ]] || fail 'choose only one of --preflight and --execute'
      MODE="${1#--}"; shift ;;
    --scenario|--snapshot-id|--profiles|--artifact|--checksum)
      [[ $# -ge 2 ]] || fail "missing value for $1"
      case "$1" in
        --scenario) SCENARIO="$2" ;;
        --snapshot-id) SNAPSHOT_ID="$2" ;;
        --profiles) PROFILE_SET="$2" ;;
        --artifact) ARTIFACT="$2" ;;
        --checksum) CHECKSUM="$2" ;;
      esac
      shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown argument: $1" ;;
  esac
done

case "$(uname -s):$(uname -m)" in
  Linux:x86_64) TARGET=x86_64-unknown-linux-gnu ;;
  Linux:aarch64) TARGET=aarch64-unknown-linux-gnu ;;
  Darwin:x86_64) TARGET=x86_64-apple-darwin ;;
  Darwin:arm64) TARGET=aarch64-apple-darwin ;;
  *) fail 'unsupported host OS/architecture for the install matrix' ;;
esac

read -r -a PROFILES <<< "$PROFILE_SET"
[[ ${#PROFILES[@]} -gt 0 ]] || fail 'choose at least one profile'
for profile in "${PROFILES[@]}"; do
  [[ "$profile" == personal || "$profile" == team ]] || fail "unknown profile: $profile"
done

if [[ "$MODE" != plan ]]; then
  [[ "$(uname -s)" == Linux ]] || fail 'macOS activation is not implemented; native Mac tests must not run this Linux installer'
  [[ ${#PROFILES[@]} -eq 1 ]] || fail 'one profile per isolated VM/snapshot; do not install personal and team sequentially'
  [[ "$SCENARIO" == fresh || "$SCENARIO" == upgrade ]] || fail 'choose --scenario fresh or upgrade'
  [[ -n "$SNAPSHOT_ID" ]] || fail 'provide --snapshot-id for the operator-created VM snapshot'
  [[ -n "$ARTIFACT" && -n "$CHECKSUM" ]] || fail 'provide a release --artifact and per-artifact --checksum; source builds are not an install test'
  [[ -f "$ARTIFACT" && -f "$CHECKSUM" ]] || fail 'release artifact or checksum does not exist'

  artifact_name="$(basename "$ARTIFACT")"
  [[ "$artifact_name" == oqto-v*"-${TARGET}.tar.gz" ]] || fail "release artifact does not match host target ${TARGET}: ${artifact_name}"
  if [[ -e "$ACTIVE_RELEASE" || -L "$ACTIVE_RELEASE" ]]; then
    [[ "$SCENARIO" == upgrade ]] || fail "fresh-install test refused: ${ACTIVE_RELEASE} already exists; reset a VM snapshot"
    [[ -d "$ACTIVE_RELEASE" ]] || fail "active release pointer is broken: ${ACTIVE_RELEASE}"
  else
    [[ "$SCENARIO" == fresh ]] || fail 'upgrade test requires an existing active release'
  fi

  # oqto-setup install currently accepts one sha256 line, not a combined
  # checksums.txt. Reject ambiguous/mismatched lines before any root action.
  mapfile -t checksum_lines < "$CHECKSUM"
  [[ ${#checksum_lines[@]} -eq 1 ]] || fail 'provide one per-artifact checksum line, not a combined checksums.txt'
  read -r expected checksum_name extra <<< "${checksum_lines[0]}"
  [[ "$expected" =~ ^[[:xdigit:]]{64}$ ]] || fail 'invalid SHA-256 checksum'
  [[ -z "${extra:-}" && -n "${checksum_name:-}" ]] || fail 'checksum must include exactly one artifact filename'
  [[ "${checksum_name#\*}" == "$artifact_name" || "${checksum_name#\*}" == "$ARTIFACT" ]] || fail 'checksum filename does not match artifact'
  actual="$(sha256sum "$ARTIFACT")"
  actual="${actual%% *}"
  [[ "${actual,,}" == "${expected,,}" ]] || fail 'release artifact checksum mismatch'

  staging="${artifact_name%.tar.gz}"
  setup_member="${staging}/bin/oqto-setup"
  member_count="$(tar -tzf "$ARTIFACT" | grep -Fxc "$setup_member" || true)"
  [[ "$member_count" == 1 ]] || fail "verified bundle must contain exactly one ${setup_member}"
  member_metadata="$(tar -tvzf "$ARTIFACT" "$setup_member")"
  [[ "${member_metadata:0:1}" == - ]] || fail 'embedded oqto-setup must be a regular file, not a link'
  printf '[matrix] read-only preflight passed: scenario=%s profile=%s target=%s snapshot=%s\n' \
    "$SCENARIO" "${PROFILES[0]}" "$TARGET" "$SNAPSHOT_ID"
fi

if [[ "$MODE" == preflight ]]; then
  exit 0
fi

if [[ "$MODE" == plan ]]; then
  printf '[matrix] read-only plan; no source build, download, sudo, service restart, or config write\n'
  if [[ -z "$ARTIFACT" || -z "$CHECKSUM" ]]; then
    printf '[matrix] obtain a target-matched release bundle and its per-artifact SHA-256 before preflight/execute\n'
  fi
  for profile in "${PROFILES[@]}"; do
    printf '[matrix] %s: take VM snapshot; preflight with --scenario fresh|upgrade --snapshot-id REF; then execute on that VM\n' "$profile"
    show_cmd oqtoctl doctor --contract --profile "$profile" --strict
  done
  exit 0
fi

# No root action above this line. Extract the matching installer only AFTER the
# bundle hash has been checked, rather than running a stale host oqto-setup.
tmpdir="$(mktemp -d)"
trap 'rm -rf -- "$tmpdir"' EXIT
staged_artifact="$tmpdir/$artifact_name"
cp -- "$ARTIFACT" "$staged_artifact"
chmod 0400 "$staged_artifact"
staged_hash="$(sha256sum "$staged_artifact")"
staged_hash="${staged_hash%% *}"
[[ "${staged_hash,,}" == "${expected,,}" ]] || fail 'staged bundle changed since preflight; refusing privileged install'
tar -xOf "$staged_artifact" "$setup_member" > "$tmpdir/oqto-setup"
chmod 0500 "$tmpdir/oqto-setup"
show_cmd sudo "$tmpdir/oqto-setup" install --artifact "$staged_artifact" --checksum "$CHECKSUM"
sudo "$tmpdir/oqto-setup" install --artifact "$staged_artifact" --checksum "$CHECKSUM"

profile="${PROFILES[0]}"
show_cmd oqtoctl doctor --contract --profile "$profile" --strict
oqtoctl doctor --contract --profile "$profile" --strict
if [[ "$profile" == personal ]]; then
  ./setup.sh --personal --doctor
else
  ./setup.sh --team --doctor
fi
printf '[matrix] %s %s install checks passed; verify runtime Pi/sandbox and rollback separately\n' "$SCENARIO" "$profile"
