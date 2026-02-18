# Sudoers Security Audit Guide

This document describes how to audit the oqto multi-user sudoers configuration for security vulnerabilities.

## Quick Audit Checklist

```bash
# 1. Verify sudoers file syntax
sudo visudo -c -f /etc/sudoers.d/oqto-multiuser

# 2. List what the oqto server user can do
sudo -l -U $(whoami)

# 3. Run the automated test suite
./scripts/test-sudoers-security.sh
```

## Threat Model

The sudoers configuration allows the oqto backend (running as `wismut`) to:
- Create Linux users with the `octo_` prefix
- Manage systemd services for those users
- Set ownership on their home directories

### Trust Boundaries

```
[Untrusted]                    [Trusted]
    |                              |
    v                              v
  Oqto Users  -->  Oqto Backend  -->  Sudoers Rules  -->  Root
    |                   |                   |
    |  (auth required)  |  (regex patterns) |
    v                   v                   v
  Web Request      API Handler         useradd/chown
```

### Attack Vectors to Test

| Vector | Description | Test Command |
|--------|-------------|--------------|
| UID injection | Create user with UID 0 (root) | `sudo useradd -u 0 -g oqto ...` |
| UID collision | Create user with existing UID | `sudo useradd -u 1000 -g oqto ...` |
| Home takeover | Chown non-oqto user's home | `sudo chown -R octo_x:oqto /home/wismut` |
| Path traversal | Escape restricted paths | `sudo mkdir /run/oqto/../../../tmp/evil` |
| Shell injection | Use dangerous shell | `sudo useradd ... -s /bin/evil ...` |
| Group escape | Use non-oqto group | `sudo useradd ... -g wheel ...` |
| Symlink attack | Chown through symlink | Create symlink, then chown |

## Automated Security Tests

Create `scripts/test-sudoers-security.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

pass() { echo -e "${GREEN}PASS${NC}: $1"; }
fail() { echo -e "${RED}FAIL${NC}: $1"; exit 1; }

echo "=== Sudoers Security Audit ==="
echo ""

# Test 1: UID 0 should be blocked
echo "Test 1: Block UID 0 (root)"
if sudo -n useradd -u 0 -g oqto -s /bin/bash -m -c test octo_evil 2>&1 | grep -q "not allowed"; then
    pass "UID 0 blocked"
else
    fail "UID 0 was allowed!"
fi

# Test 2: UID 1000 should be blocked
echo "Test 2: Block UID 1000 (typical user)"
if sudo -n useradd -u 1000 -g oqto -s /bin/bash -m -c test octo_evil 2>&1 | grep -q "not allowed"; then
    pass "UID 1000 blocked"
else
    fail "UID 1000 was allowed!"
fi

# Test 3: Non-oqto prefix should be blocked
echo "Test 3: Block non-octo_ username prefix"
if sudo -n useradd -u 2000 -g oqto -s /bin/bash -m -c test evil_user 2>&1 | grep -q "not allowed"; then
    pass "Non-oqto prefix blocked"
else
    fail "Non-oqto prefix was allowed!"
fi

# Test 4: Chown other user's home should be blocked
echo "Test 4: Block chown on other user's home"
if sudo -n chown -R octo_test:oqto /home/wismut 2>&1 | grep -q "not allowed"; then
    pass "Chown /home/wismut blocked"
else
    fail "Chown /home/wismut was allowed!"
fi

# Test 5: Path traversal should be blocked
echo "Test 5: Block path traversal in mkdir"
if sudo -n mkdir -p /run/oqto/runner-sockets/octo_../../tmp/evil 2>&1 | grep -q "not allowed"; then
    pass "Path traversal blocked"
else
    fail "Path traversal was allowed!"
fi

# Test 6: Non-bash shell should be blocked
echo "Test 6: Block non-allowed shells"
if sudo -n useradd -u 2000 -g oqto -s /bin/zsh -m -c test octo_test 2>&1 | grep -q "not allowed"; then
    pass "Non-bash shell blocked"
else
    fail "Non-bash shell was allowed!"
fi

# Test 7: Non-oqto group should be blocked
echo "Test 7: Block non-oqto group"
if sudo -n useradd -u 2000 -g wheel -s /bin/bash -m -c test octo_test 2>&1 | grep -q "not allowed"; then
    pass "Non-oqto group blocked"
else
    fail "Non-oqto group was allowed!"
fi

# Test 8: Valid command should work
echo "Test 8: Valid useradd command format"
# Don't actually create user, just check if it would be allowed
if sudo -n -l useradd -u 2000 -g oqto -s /bin/bash -m -c testuser octo_testuser 2>&1 | grep -q "useradd"; then
    pass "Valid useradd would be allowed"
else
    fail "Valid useradd was blocked!"
fi

echo ""
echo "=== All security tests passed ==="
```

## Manual Audit Steps

### 1. Review Regex Patterns

For each Cmnd_Alias, verify the regex:

```bash
# Extract patterns from sudoers
grep "^Cmnd_Alias" /etc/sudoers.d/oqto-multiuser
```

Check each pattern against:
- Does `[a-z0-9_]` exclude `/`, `..`, spaces?
- Does UID pattern `[2-9][0-9][0-9][0-9]` exclude 0-1999?
- Are all arguments anchored with `^...$`?

### 2. Test Edge Cases

```bash
# Test with special characters in comment field
sudo useradd -u 2000 -g oqto -s /bin/bash -m -c "test;id" octo_test

# Test with unicode
sudo useradd -u 2000 -g oqto -s /bin/bash -m -c "test" octo_tëst

# Test maximum length username
sudo useradd -u 2000 -g oqto -s /bin/bash -m -c "test" octo_$(printf 'a%.0s' {1..32})
```

### 3. Verify Backend Matches Sudoers

Compare the actual commands the backend runs against sudoers patterns:

```bash
# Enable debug logging in backend
RUST_LOG=debug cargo run

# Create a user via API and capture the useradd command
# Verify it matches the sudoers regex exactly
```

### 4. Check for TOQTOU Vulnerabilities

Time-of-check to time-of-use vulnerabilities in chown:

```bash
# In one terminal, create a race condition target
mkdir /home/octo_test/workspace
ln -s /etc/passwd /home/octo_test/workspace/link

# The -R flag follows symlinks by default!
# Consider using --no-dereference or avoiding -R
```

## External Security Review

For production deployments, consider:

1. **Penetration testing** - Hire security firm to test multi-user isolation
2. **Code review** - Review linux_users.rs for command injection
3. **Fuzzing** - Fuzz the API endpoints that trigger sudo commands
4. **Audit logging** - Enable sudo logging and monitor for anomalies

```bash
# Enable sudo logging
echo "Defaults log_output" | sudo tee -a /etc/sudoers.d/oqto-logging
echo "Defaults!/usr/bin/sudoreplay !log_output" | sudo tee -a /etc/sudoers.d/oqto-logging
```

## Known Limitations

1. **Regex requires sudo 1.9.10+** - Older systems need wildcard patterns (less secure)
2. **-R flag in chown** - Could follow symlinks; consider `--no-dereference`
3. **Comment field** - Allows arbitrary text (except colons); sanitized in backend
4. **No rate limiting** - Sudoers doesn't limit how many users can be created

## References

- [sudoers(5) man page](https://www.sudo.ws/docs/man/sudoers.man/)
- [MITRE ATT&CK - Sudo and Sudo Caching](https://attack.mitre.org/techniques/T1548/003/)
- [CIS Benchmark for sudo](https://www.cisecurity.org/benchmark/distribution_independent_linux)
