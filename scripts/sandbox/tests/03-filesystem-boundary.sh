#!/usr/bin/env bash
# Scenario 03: filesystem boundary.
# Executes real commands inside the sandbox and verifies:
#   - workspace is writable
#   - allow_write paths are writable
#   - deny_read paths are unreadable
#   - paths outside allow_write but inside home are read-only
#   - deny_write paths deny write even when readable

set -euo pipefail
SCENARIO_NAME="03-filesystem-boundary"
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
trap cleanup_run_dir EXIT
scenario_header
require_oqto_sandbox
have_bwrap || { skip "bwrap missing"; scenario_summary; exit 0; }

ws="$(make_workspace fs)"
writable_dir="${OQTO_TEST_RUN_DIR}/wx-allowed"
readonly_dir="${OQTO_TEST_RUN_DIR}/wx-readonly"
secret_dir="${OQTO_TEST_RUN_DIR}/wx-secret"
mkdir -p "${writable_dir}" "${readonly_dir}" "${secret_dir}"
echo "public-content" >"${readonly_dir}/file.txt"
echo "secret-content" >"${secret_dir}/top-secret.txt"

cfg="${OQTO_TEST_RUN_DIR}/fs.toml"
write_toml "${cfg}" <<EOF
enabled = true
profile = "fs-test"

[profiles.fs-test]
deny_read = ["${secret_dir}"]
allow_write = ["${writable_dir}", "/tmp"]
deny_write = []
isolate_network = false
isolate_pid = true
drop_all_caps = false
disable_userns = false
assert_userns_disabled = false
no_new_privs = false
seccomp_mode = "off"
landlock_mode = "off"
EOF

sb() { run_sandbox --config "${cfg}" --workspace "${ws}" -- "$@"; }

# --- positive: workspace writable ---
assert_success "workspace writable" -- sb sh -c "touch '${ws}/ok' && test -f '${ws}/ok'"

# --- positive: allow_write dir writable ---
assert_success "allow_write path writable" -- sb sh -c "touch '${writable_dir}/ok' && test -f '${writable_dir}/ok'"

# --- positive: /tmp writable ---
assert_success "/tmp writable" -- sb sh -c "touch /tmp/oqto-fs-test-ok && rm /tmp/oqto-fs-test-ok"

# --- negative: deny_read blocks read ---
assert_failure "deny_read path unreadable" -- sb cat "${secret_dir}/top-secret.txt"

# --- negative: readonly path read-ok, write-denied ---
assert_success  "readable-but-not-writable path is readable" -- sb cat "${readonly_dir}/file.txt"
assert_failure  "readable-but-not-writable path rejects writes" -- sb sh -c "touch '${readonly_dir}/should-fail'"

# --- negative: home secrets (ssh) always denied under dev/strict ---
# Use a fresh config that just uses the `development` preset and assert the
# user's real ~/.ssh listing fails if it exists on disk.
if [[ -d "${HOME}/.ssh" ]]; then
  cfg_dev="${OQTO_TEST_RUN_DIR}/dev.toml"
  write_toml "${cfg_dev}" <<EOF
enabled = true
profile = "development"
EOF
  assert_failure "~/.ssh unreadable under development profile" -- \
    "${OQTO_SANDBOX_BIN_RESOLVED}" --config "${cfg_dev}" --workspace "${ws}" -- \
    sh -c "ls ${HOME}/.ssh >/dev/null 2>&1 && test \"\$(ls -A ${HOME}/.ssh 2>/dev/null)\" != ''"
else
  skip "host has no ~/.ssh; cannot verify secrets deny path"
fi

scenario_summary
