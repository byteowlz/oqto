#!/usr/bin/env bash
set -euo pipefail

# Configuration - adjust these if needed
SERVER_USER="${USER}"           # User running the octo server
USER_PREFIX="octo_"             # Prefix for managed Linux users
OCTO_GROUP="octo"               # Shared group for octo users
UID_START="${OCTO_UID_START:-2000}"
UID_FIRST_DIGIT="${UID_START:0:1}"

SUDOERS_FILE="/etc/sudoers.d/octo-multiuser"

cat << EOF | sudo tee "$SUDOERS_FILE" > /dev/null
# Octo Multi-User Process Isolation - SECURE VERSION
# Generated on $(date)
# Allows the octo server user to manage isolated user accounts
#
# SECURITY: Uses regex patterns (^...\$) to prevent privilege escalation.
# - UIDs restricted to ${UID_FIRST_DIGIT}000-${UID_FIRST_DIGIT}999 range (avoids system/user UIDs)
# - Usernames must start with ${USER_PREFIX} prefix
# - Workspace chown restricted to ${USER_PREFIX}* home directories only
# Requires sudo 1.9.10+ for regex support.

# Group management - only create the ${OCTO_GROUP} group (safe - fixed value)
Cmnd_Alias OCTO_GROUPADD = /usr/sbin/groupadd ${OCTO_GROUP}

# User creation - RESTRICTED to safe UID range and ${USER_PREFIX} prefix
# Regex matches: -u NNNN -g ${OCTO_GROUP} -s /bin/bash -m/-M -c COMMENT USERNAME
# UID must be ${UID_FIRST_DIGIT}000-${UID_FIRST_DIGIT}999, username must start with ${USER_PREFIX}
# GECOS format: "Octo platform user: <user_id>" - use .* to match including spaces
Cmnd_Alias OCTO_USERADD = \\
    /usr/sbin/useradd ^-u [${UID_FIRST_DIGIT}][0-9][0-9][0-9] -g ${OCTO_GROUP} -s /bin/bash -m -c .* ${USER_PREFIX}[a-z0-9_]+\$, \\
    /usr/sbin/useradd ^-u [${UID_FIRST_DIGIT}][0-9][0-9][0-9] -g ${OCTO_GROUP} -s /bin/bash -M -c .* ${USER_PREFIX}[a-z0-9_]+\$

# User deletion - only ${USER_PREFIX} users, no home removal (-r flag not allowed)
Cmnd_Alias OCTO_USERDEL = /usr/sbin/userdel ^${USER_PREFIX}[a-z0-9_]+\$

# Directory creation for runner sockets - RESTRICTED path (no path traversal)
Cmnd_Alias OCTO_MKDIR = /bin/mkdir ^-p /run/octo/runner-sockets/${USER_PREFIX}[a-z0-9_]+\$

# Runner socket ownership - RESTRICTED to exact paths
Cmnd_Alias OCTO_CHOWN_RUNNER = \\
    /usr/bin/chown ^${USER_PREFIX}[a-z0-9_]+\\:${OCTO_GROUP} /run/octo/runner-sockets/${USER_PREFIX}[a-z0-9_]+\$

# Workspace ownership - RESTRICTED to ${USER_PREFIX} user home directories ONLY
# SECURITY: Only allows chown on /home/${USER_PREFIX}*/... NOT on other users' homes
# The regex ensures the path starts with /home/${USER_PREFIX} to prevent privilege escalation
Cmnd_Alias OCTO_CHOWN_WORKSPACE = \\
    /usr/bin/chown ^-R ${USER_PREFIX}[a-z0-9_]+\\:${OCTO_GROUP} /home/${USER_PREFIX}[a-z0-9_]+(/[^.][^/]*)*\$

# Permissions for runner socket directories
Cmnd_Alias OCTO_CHMOD_RUNNER = /usr/bin/chmod ^2770 /run/octo/runner-sockets/${USER_PREFIX}[a-z0-9_]+\$

# systemd linger - only for ${USER_PREFIX} users
Cmnd_Alias OCTO_LINGER = /usr/bin/loginctl ^enable-linger ${USER_PREFIX}[a-z0-9_]+\$

# Start user systemd instance - RESTRICTED to ${USER_PREFIX} user UIDs
Cmnd_Alias OCTO_START_USER = /usr/bin/systemctl ^start user@[${UID_FIRST_DIGIT}][0-9][0-9][0-9]\\.service\$

# User management - group and user creation
${SERVER_USER} ALL=(root) NOPASSWD: OCTO_GROUPADD, OCTO_USERADD

# systemd user management - enable/start octo-runner as ${USER_PREFIX}* users
Cmnd_Alias OCTO_RUNNER_SYSTEMCTL = \\
    /usr/bin/systemctl --user enable --now octo-runner, \\
    /usr/bin/systemctl --user start octo-runner, \\
    /usr/bin/systemctl --user enable octo-runner
${SERVER_USER} ALL=(${USER_PREFIX}*) NOPASSWD: OCTO_RUNNER_SYSTEMCTL

# Runner socket directory setup and workspace ownership
${SERVER_USER} ALL=(root) NOPASSWD: OCTO_MKDIR, OCTO_CHOWN_RUNNER, OCTO_CHOWN_WORKSPACE, OCTO_CHMOD_RUNNER

# User systemd management
${SERVER_USER} ALL=(root) NOPASSWD: OCTO_START_USER, OCTO_LINGER
EOF

sudo chmod 440 "$SUDOERS_FILE"

# Validate
if sudo visudo -c -f "$SUDOERS_FILE"; then
    echo "Sudoers file updated successfully: $SUDOERS_FILE"
else
    echo "ERROR: Invalid sudoers file - removing it"
    sudo rm -f "$SUDOERS_FILE"
    exit 1
fi
