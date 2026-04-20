#!/usr/bin/env bash
# Scenario 09: workspace sandbox config merge rules.
#
# Global config.toml for this test allows network and denies only /tmp/secret-global.
# Workspace .oqto/sandbox.toml adds /tmp/secret-ws to deny_read and flips
# isolate_network=true. Merge rules state:
#   - deny_read: union    -> both paths denied
#   - isolate_network: OR -> must end up true
#   - allow_write: intersection -> must contain only values allowed by BOTH
#
# We verify by inspecting --dry-run bwrap output.

set -euo pipefail
SCENARIO_NAME="09-workspace-merge"
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
trap cleanup_run_dir EXIT
scenario_header
require_oqto_sandbox
have_bwrap || { skip "bwrap missing"; scenario_summary; exit 0; }

ws="$(make_workspace merge)"
mkdir -p "${ws}/.oqto"
mkdir -p /tmp/.oqto-secret-global /tmp/.oqto-secret-ws

global_cfg="${OQTO_TEST_RUN_DIR}/global.toml"
write_toml "${global_cfg}" <<EOF
enabled = true
profile = "merge-global"

[profiles.merge-global]
deny_read = ["/tmp/.oqto-secret-global"]
allow_write = ["/tmp", "${HOME}/.cargo"]
deny_write = []
isolate_network = false
isolate_pid = false
drop_all_caps = false
disable_userns = false
assert_userns_disabled = false
no_new_privs = false
seccomp_mode = "off"
landlock_mode = "off"
EOF

write_toml "${ws}/.oqto/sandbox.toml" <<EOF
deny_read = ["/tmp/.oqto-secret-ws"]
allow_write = ["/tmp"]
isolate_network = true
EOF

out="$(run_sandbox --config "${global_cfg}" --workspace "${ws}" --dry-run -- /bin/true 2>&1)"

case "${out}" in
  *"/tmp/.oqto-secret-global"*) pass "deny_read union preserves global path";;
  *) fail "deny_read missing global path";;
esac
case "${out}" in
  *"/tmp/.oqto-secret-ws"*) pass "deny_read union adds workspace path";;
  *) fail "deny_read missing workspace path";;
esac
case "${out}" in
  *"--unshare-net"*) pass "isolate_network OR -> --unshare-net present";;
  *) fail "isolate_network not unioned (expected --unshare-net)";;
esac

# Intersection check on allow_write: ~/.cargo is only in global, workspace dropped it.
# After merge, the path should NOT be rw-bound.
if [[ "${out}" == *" ${HOME}/.cargo "* ]]; then
  fail "allow_write intersection did not drop ~/.cargo"
else
  pass "allow_write intersection drops paths missing from workspace"
fi

# /tmp is in both global and workspace allow_write -- must survive.
case "${out}" in
  *"/tmp"*) pass "allow_write intersection keeps shared path /tmp";;
  *) fail "allow_write intersection dropped /tmp";;
esac

scenario_summary
