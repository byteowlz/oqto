#!/usr/bin/env bash
# Sudoers Security Audit Script for Octo Multi-User Mode
# Tests that sudoers regex patterns properly restrict privilege escalation
#
# NOTE: If you have full sudo access (e.g., "wismut ALL=(ALL) ALL"), all commands
# will be allowed at runtime. This script tests the PATTERNS themselves to verify
# they would restrict a dedicated service user that ONLY has these rules.
set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Counters
PASS=0
FAIL=0

pass() { 
    echo -e "${GREEN}PASS${NC}: $1"
    ((PASS++)) || true
}

fail() { 
    echo -e "${RED}FAIL${NC}: $1"
    ((FAIL++)) || true
}

# Verify sudoers rules are active by checking sudo -l output
SUDOERS_FILE="/etc/sudoers.d/octo-multiuser"
if ! sudo -l 2>/dev/null | grep -qE "useradd|NOPASSWD"; then
    echo -e "${RED}ERROR${NC}: Cannot verify sudoers rules. Make sure $SUDOERS_FILE exists."
    echo "Install it with:"
    echo "  sudo cp /tmp/octo-multiuser.sudoers $SUDOERS_FILE"
    echo "  sudo chmod 440 $SUDOERS_FILE"
    exit 1
fi

echo "=== Octo Sudoers Security Audit ==="
echo "Testing regex patterns in: $SUDOERS_FILE"
echo ""

# Check if user has unrestricted sudo access
if sudo -l 2>/dev/null | grep -qE "\(ALL\).*ALL|\(ALL : ALL\).*ALL"; then
    echo -e "${YELLOW}NOTE${NC}: You have full sudo access, so runtime tests would pass everything."
    echo "This script tests the REGEX PATTERNS to verify they would restrict a"
    echo "dedicated service user that only has the octo-multiuser rules."
    echo ""
fi

# The allowed patterns from our sudoers file (for useradd with -m flag):
# ^-u [2][0-9][0-9][0-9] -g octo -s /bin/bash -m -c [^ ]+ octo_[a-z0-9_]+$
USERADD_PATTERN='^-u [2][0-9][0-9][0-9] -g octo -s /bin/bash -m -c [^ ]+ octo_[a-z0-9_]+$'

# The allowed pattern for chown workspace:
# ^-R octo_[a-z0-9_]+\:octo /home/octo_[a-z0-9_]+(/[^.][^/]*)*$
CHOWN_PATTERN='^-R octo_[a-z0-9_]+:octo /home/octo_[a-z0-9_]+(/[^.][^/]*)*$'

# The allowed pattern for mkdir:
# ^-p /run/octo/runner-sockets/octo_[a-z0-9_]+$
MKDIR_PATTERN='^-p /run/octo/runner-sockets/octo_[a-z0-9_]+$'

# The allowed pattern for systemctl start:
# ^start user@[2][0-9][0-9][0-9]\.service$
SYSTEMCTL_PATTERN='^start user@[2][0-9][0-9][0-9]\.service$'

# Test if args would be BLOCKED (should NOT match the allowed pattern)
test_useradd_blocked() {
    local desc="$1"
    local args="$2"
    
    if echo "$args" | grep -qE "$USERADD_PATTERN"; then
        fail "$desc"
        echo "       Args: $args"
        echo "       Pattern matched - would be ALLOWED"
    else
        pass "$desc"
    fi
}

# Test if args would be ALLOWED (should match the pattern)
test_useradd_allowed() {
    local desc="$1"
    local args="$2"
    
    if echo "$args" | grep -qE "$USERADD_PATTERN"; then
        pass "$desc"
    else
        fail "$desc"
        echo "       Args: $args"
        echo "       Pattern did NOT match - would be BLOCKED"
    fi
}

test_chown_blocked() {
    local desc="$1"
    local args="$2"
    
    if echo "$args" | grep -qE "$CHOWN_PATTERN"; then
        fail "$desc"
        echo "       Args: $args"
    else
        pass "$desc"
    fi
}

test_mkdir_blocked() {
    local desc="$1"
    local args="$2"
    
    if echo "$args" | grep -qE "$MKDIR_PATTERN"; then
        fail "$desc"
        echo "       Args: $args"
    else
        pass "$desc"
    fi
}

test_systemctl_blocked() {
    local desc="$1"
    local args="$2"
    
    if echo "$args" | grep -qE "$SYSTEMCTL_PATTERN"; then
        fail "$desc"
        echo "       Args: $args"
    else
        pass "$desc"
    fi
}

echo "--- USERADD: UID Injection Tests ---"
echo "(These should be BLOCKED - not match the allowed pattern)"
echo ""

test_useradd_blocked "Block UID 0 (root)" \
    "-u 0 -g octo -s /bin/bash -m -c test octo_evil"

test_useradd_blocked "Block UID 1 (daemon)" \
    "-u 1 -g octo -s /bin/bash -m -c test octo_evil"

test_useradd_blocked "Block UID 1000 (typical user)" \
    "-u 1000 -g octo -s /bin/bash -m -c test octo_evil"

test_useradd_blocked "Block UID 1999 (below range)" \
    "-u 1999 -g octo -s /bin/bash -m -c test octo_evil"

test_useradd_blocked "Block UID 10000 (above range)" \
    "-u 10000 -g octo -s /bin/bash -m -c test octo_evil"

echo ""
echo "--- USERADD: Username Prefix Tests ---"
echo ""

test_useradd_blocked "Block non-octo_ prefix (evil_)" \
    "-u 2000 -g octo -s /bin/bash -m -c test evil_user"

test_useradd_blocked "Block no prefix" \
    "-u 2000 -g octo -s /bin/bash -m -c test alice"

test_useradd_blocked "Block root username" \
    "-u 2000 -g octo -s /bin/bash -m -c test root"

echo ""
echo "--- USERADD: Group Restriction Tests ---"
echo ""

test_useradd_blocked "Block wheel group" \
    "-u 2000 -g wheel -s /bin/bash -m -c test octo_test"

test_useradd_blocked "Block root group" \
    "-u 2000 -g root -s /bin/bash -m -c test octo_test"

test_useradd_blocked "Block sudo group" \
    "-u 2000 -g sudo -s /bin/bash -m -c test octo_test"

echo ""
echo "--- USERADD: Shell Restriction Tests ---"
echo ""

test_useradd_blocked "Block /bin/sh shell" \
    "-u 2000 -g octo -s /bin/sh -m -c test octo_test"

test_useradd_blocked "Block /usr/bin/zsh shell" \
    "-u 2000 -g octo -s /usr/bin/zsh -m -c test octo_test"

test_useradd_blocked "Block /bin/false shell" \
    "-u 2000 -g octo -s /bin/false -m -c test octo_test"

echo ""
echo "--- USERADD: Valid Commands (should be ALLOWED) ---"
echo ""

test_useradd_allowed "Allow valid: UID 2000, octo group, bash, octo_ prefix" \
    "-u 2000 -g octo -s /bin/bash -m -c testuser octo_alice"

test_useradd_allowed "Allow valid: UID 2999, octo group, bash, octo_ prefix" \
    "-u 2999 -g octo -s /bin/bash -m -c testuser octo_bob"

test_useradd_allowed "Allow valid: UID 2500, longer username" \
    "-u 2500 -g octo -s /bin/bash -m -c testuser octo_user_with_underscores"

echo ""
echo "--- CHOWN: Path Security Tests ---"
echo "(These should be BLOCKED)"
echo ""

test_chown_blocked "Block chown /home/wismut (other user)" \
    "-R octo_test:octo /home/wismut"

test_chown_blocked "Block chown /home/root" \
    "-R octo_test:octo /home/root"

test_chown_blocked "Block chown /etc" \
    "-R octo_test:octo /etc"

test_chown_blocked "Block chown /root" \
    "-R octo_test:octo /root"

test_chown_blocked "Block chown with path traversal" \
    "-R octo_test:octo /home/octo_test/../wismut"

test_chown_blocked "Block chown hidden dir at root" \
    "-R octo_test:octo /home/octo_test/.ssh"

echo ""
echo "--- MKDIR: Path Traversal Tests ---"
echo "(These should be BLOCKED)"
echo ""

test_mkdir_blocked "Block mkdir path traversal" \
    "-p /run/octo/runner-sockets/octo_test/../../../tmp/evil"

test_mkdir_blocked "Block mkdir outside /run/octo" \
    "-p /tmp/octo_test"

test_mkdir_blocked "Block mkdir for non-octo user" \
    "-p /run/octo/runner-sockets/evil_user"

echo ""
echo "--- SYSTEMCTL: UID Restriction Tests ---"
echo "(These should be BLOCKED)"
echo ""

test_systemctl_blocked "Block systemctl for UID 0" \
    "start user@0.service"

test_systemctl_blocked "Block systemctl for UID 1000" \
    "start user@1000.service"

test_systemctl_blocked "Block systemctl for UID 1999" \
    "start user@1999.service"

test_systemctl_blocked "Block systemctl stop (only start allowed)" \
    "stop user@2000.service"

echo ""
echo "=== Summary ==="
echo -e "Passed: ${GREEN}$PASS${NC}"
echo -e "Failed: ${RED}$FAIL${NC}"
echo ""

if [[ $FAIL -gt 0 ]]; then
    echo -e "${RED}SECURITY AUDIT FAILED${NC} - $FAIL pattern vulnerabilities found!"
    echo "The sudoers regex patterns need to be fixed."
    exit 1
else
    echo -e "${GREEN}SECURITY AUDIT PASSED${NC}"
    echo "All regex patterns correctly restrict dangerous commands."
    exit 0
fi
