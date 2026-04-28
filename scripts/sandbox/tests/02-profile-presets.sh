#!/usr/bin/env bash
# Scenario 02: profile presets (minimal / development / strict) via --dry-run.
#
# We assert the generated bwrap arg list contains (or does not contain) specific
# flags that prove each preset's hardening knobs are wired up correctly.
#
# Uses --dry-run so nothing actually executes; safe on any host.

set -euo pipefail
SCENARIO_NAME="02-profile-presets"
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
trap cleanup_run_dir EXIT
scenario_header
require_oqto_sandbox

ws="$(make_workspace presets)"

dry() {
  # $1 = profile name, emits the dry-run bwrap args on stdout.
  local profile="$1"
  local cfg="${OQTO_TEST_RUN_DIR}/preset-${profile}.toml"
  write_toml "${cfg}" <<EOF
enabled = true
profile = "${profile}"
EOF
  run_sandbox --config "${cfg}" --workspace "${ws}" --dry-run -- /bin/true 2>&1
}

# ----- development (default) -----
info "development preset"
out_dev="$(dry development)"
case "${out_dev}" in
  *"--unshare-pid"*) pass "development has --unshare-pid (isolate_pid=true)";;
  *) fail "development missing --unshare-pid";;
esac
case "${out_dev}" in
  *"--disable-userns"*) pass "development has --disable-userns";;
  *) fail "development missing --disable-userns";;
esac
case "${out_dev}" in
  *"--unshare-net"*) fail "development unexpectedly has --unshare-net";;
  *) pass "development does not set --unshare-net (isolate_network=false)";;
esac

# ----- strict -----
info "strict preset"
out_strict="$(dry strict)"
case "${out_strict}" in
  *"--unshare-net"*) pass "strict has --unshare-net (isolate_network=true)";;
  *) fail "strict missing --unshare-net";;
esac
case "${out_strict}" in
  *"--cap-drop"*"ALL"*) pass "strict drops all capabilities";;
  *) fail "strict missing --cap-drop ALL";;
esac
case "${out_strict}" in
  *"--assert-userns-disabled"*) pass "strict asserts userns disabled";;
  *) fail "strict missing --assert-userns-disabled";;
esac

# ----- minimal -----
info "minimal preset"
out_min="$(dry minimal)"
case "${out_min}" in
  *"--unshare-net"*) fail "minimal unexpectedly has --unshare-net";;
  *) pass "minimal does not isolate network";;
esac
case "${out_min}" in
  *"--unshare-pid"*) fail "minimal unexpectedly has --unshare-pid";;
  *) pass "minimal does not isolate pid";;
esac

# ----- deny_read secrets protected in every preset -----
for p in minimal development strict; do
  out="$(dry "${p}")"
  case "${out}" in
    *".ssh"*) pass "${p} protects ~/.ssh";;
    *) fail "${p} does not mention ~/.ssh in deny list";;
  esac
done

scenario_summary
