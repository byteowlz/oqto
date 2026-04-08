#!/usr/bin/env bash
# fix-sudo-pam.sh — replace /etc/pam.d/sudo with a stack that does NOT use
# pam_faillock, so TTY-less sudo attempts from agents can't lock out the user.
#
# Login via getty/SSH still goes through system-auth and remains faillock-protected.
#
# Safety:
#   - Backs up the existing file with a timestamp.
#   - Writes atomically via a temp file + mv.
#   - Prints a verification step; DO NOT close this shell until you have
#     confirmed `sudo -v` works in a SECOND terminal.
#   - To roll back:  sudo cp /etc/pam.d/sudo.bak.<timestamp> /etc/pam.d/sudo

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "Re-executing under sudo..."
    exec sudo -- "$0" "$@"
fi

TARGET=/etc/pam.d/sudo
STAMP=$(date +%Y%m%d-%H%M%S)
BACKUP="${TARGET}.bak.${STAMP}"
TMP=$(mktemp "${TARGET}.new.XXXXXX")
trap 'rm -f "$TMP"' EXIT

if [[ ! -f "$TARGET" ]]; then
    echo "ERROR: $TARGET does not exist" >&2
    exit 1
fi

cp -a "$TARGET" "$BACKUP"
echo "Backed up $TARGET -> $BACKUP"

cat > "$TMP" <<'PAM'
#%PAM-1.0
# Managed by fix-sudo-pam.sh
# Intentionally does NOT include system-auth for the auth phase, so that
# pam_faillock cannot count TTY-less PAM conversation failures (from agent
# sessions without a controlling terminal) as failed passwords and lock the
# account. Interactive logins (getty, SSH, GDM/SDDM) still use system-auth
# and remain protected by faillock.
auth       [success=2 default=ignore]  pam_unix.so          try_first_pass nullok
auth       [success=1 default=bad]     pam_systemd_home.so
auth       optional                    pam_permit.so
auth       required                    pam_env.so
account    include                     system-auth
session    include                     system-auth
PAM

chmod 644 "$TMP"
chown root:root "$TMP"
mv "$TMP" "$TARGET"
trap - EXIT

echo
echo "New $TARGET:"
echo "----------------------------------------"
cat "$TARGET"
echo "----------------------------------------"

# Clear any lingering faillock state for the invoking user.
REAL_USER="${SUDO_USER:-$USER}"
if command -v faillock >/dev/null 2>&1; then
    faillock --user "$REAL_USER" --reset || true
    echo "Cleared faillock state for $REAL_USER"
fi

cat <<EOF

NEXT STEP — verify in a SEPARATE terminal before closing this shell:

    sudo -k            # forget cached credential
    sudo -v            # should prompt and accept your password

If that works, you're done. If it does NOT work, roll back with:

    sudo cp $BACKUP $TARGET

EOF
