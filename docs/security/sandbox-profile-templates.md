# Sandbox Profile Templates (Shipped)

Oqto ships ready-to-use sandbox templates in:

- `backend/crates/oqto/examples/sandbox.template.strict-infra.toml`
- `backend/crates/oqto/examples/sandbox.template.development-safe.toml`
- `backend/crates/oqto/examples/sandbox.template.networked-dev.toml`

## Recommended usage

- **strict-infra**: security/control-plane repos (`oqto`, `kyz`, `eavs`, `hstry`, `mmry`, etc.)
- **development-safe**: general coding repos with package manager/toolchain writes
- **networked-dev**: repos that require broad external network access during development

## Rollout advice

Start with:

- `seccomp_mode = "audit"`
- `landlock_mode = "audit"`

Then move to `"enforce"` per repo after validating workloads.

For seccomp enforce mode, install an architecture-specific BPF artifact:

```bash
sudo ./scripts/sandbox/install-seccomp-policy.sh
```

Policy source is shipped at:
- `backend/crates/oqto/examples/seccomp/default.policy.toml`

## Install as system policy

```bash
sudo cp backend/crates/oqto/examples/sandbox.template.strict-infra.toml /etc/oqto/sandbox.toml
sudo chown root:oqto /etc/oqto/sandbox.toml
sudo chmod 640 /etc/oqto/sandbox.toml
```

For multi-user/runner-isolated mode, `/etc/oqto/sandbox.toml` is the trusted global policy path.
