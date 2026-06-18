# Oqto install contract

This document captures the current setup contract used by `oqto-setup plan` and `oqtoctl doctor --contract`.

The source of truth is the typed manifest in `backend/crates/oqto-provisioning`. Regenerate/verify with:

```bash
oqto-setup plan --profile personal
oqto-setup plan --profile team
./setup.sh --team --doctor --json
```

## Personal profile

```text
Oqto setup plan: Personal single-user local install
Runner socket: /run/user/{uid}/oqto-runner.sock

Paths:
- $XDG_CONFIG_HOME/oqto/config.toml owner=$USER group=$USER_PRIMARY_GROUP mode=0600 -- backend configuration
- $XDG_RUNTIME_DIR/oqto-runner.sock owner=$USER group=$USER_PRIMARY_GROUP mode=0600 -- single-user runner RPC socket

Services:
- oqto-runner.service user=$USER enabled=true active=true -- single-user runner daemon
- oqto.service user=$USER enabled=true active=true -- Oqto backend
- eavs.service user=$USER enabled=true active=true -- LLM proxy

Declared checks (static; severity shown only if the check fails):
- severity-if-failed=Error: oqto-runner socket exists and accepts RPC -- remediation: start/restart the user oqto-runner.service
- severity-if-failed=Error: Pi models.json is generated from EAVS -- remediation: run setup config sync or regenerate EAVS models
```

## Team profile

```text
Oqto setup plan: Team multi-user Linux install with per-user runners
Runner socket: /run/oqto/runner-sockets/{user}/oqto-runner.sock

Paths:
- /etc/oqto owner=root group=root mode=0755 -- system policy/config directory
- /var/lib/oqto owner=oqto group=oqto mode=0755 -- backend service state
- /run/oqto/runner-sockets owner=root group=oqto mode=2770 -- shared parent for per-user runner sockets
- /etc/sudoers.d/oqto-multiuser owner=root group=root mode=0440 -- sudoers policy for safe multi-user provisioning
- /run/oqto/runner-sockets/{linux_username}/oqto-runner.sock owner={linux_username} group=oqto mode=0660 -- canonical per-user runner RPC socket

Services:
- oqto.service user=root/system enabled=true active=true -- Oqto backend
- eavs.service user=root/system enabled=true active=true -- LLM proxy
- oqto-runner.service user={linux_username} enabled=true active=true -- per-user runner daemon

Declared checks (static; severity shown only if the check fails):
- severity-if-failed=Error: each active Oqto user has matching linux_username/linux_uid and OS account -- remediation: run oqtoctl doctor --apply or reprovision the user
- severity-if-failed=Error: each active user's runner socket exists at the canonical shared path -- remediation: restart/reprovision the per-user oqto-runner service
- severity-if-failed=Warning: user-runtime and shared runner sockets are not both present for the same user -- remediation: remove stale runner units or align runner_socket_pattern
- severity-if-failed=Error: /etc/sudoers.d/oqto-multiuser validates with visudo -- remediation: regenerate Linux user isolation sudoers rules
```

## Doctor and remediation commands

Use these commands instead of manually guessing paths, owners, permissions, or services:

```bash
# Read-only contract preview
./setup.sh --team --plan
./setup.sh --team --plan --json

# Read-only drift report
./setup.sh --team --doctor
./setup.sh --team --doctor --json

# Strict preflight/CI gate
./setup.sh --team --doctor --strict

# Scoped apply modes
sudo ./setup.sh --team --doctor --apply
sudo ./setup.sh --team --doctor --apply --apply-runners
sudo ./setup.sh --team --doctor --apply --apply-services
```

Apply scopes are deliberately explicit:

- `--apply`: low-risk shared runner socket directory repair.
- `--apply --apply-runners`: also reprovision per-user runner units with socket drift.
- `--apply --apply-services`: also enable/start declared system services.
