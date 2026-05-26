# Oqto Seccomp Artifacts

This directory contains seccomp policy sources and (optionally) precompiled BPF artifacts.

## Files

- `default.policy.toml` - canonical human-readable policy source.
- `default-x86_64.bpf` - precompiled BPF artifact for x86_64 (to be shipped).
- `default-aarch64.bpf` - precompiled BPF artifact for aarch64 (to be shipped).

## Generate artifacts

```bash
./scripts/sandbox/generate-seccomp-artifacts.sh
```

This produces:

- `default-x86_64.bpf`
- `default-aarch64.bpf`

## Runtime usage

Set in sandbox config:

```toml
seccomp_mode = "enforce"
seccomp_bpf_path = "/usr/local/share/oqto/seccomp/default.bpf"
```

Use `audit` first during rollout.

## Install helper

```bash
sudo ./scripts/sandbox/install-seccomp-policy.sh
```

This picks the architecture-specific artifact and installs to:

- `/usr/local/share/oqto/seccomp/default.bpf`
- compatibility symlink: `/etc/oqto/seccomp/default.bpf`
