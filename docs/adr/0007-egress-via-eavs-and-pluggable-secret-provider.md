# Egress control is eavs; secret backends are pluggable

Agent network isolation is built as three separable layers, not one bespoke proxy. We will not build a second egress proxy: eavs already ships the general-purpose one (`eavs/src/egress.rs` — a transparent firewall that takes redirected connections over PROXY protocol v2, peeks TLS SNI / HTTP Host to drive a domain ACL with enforce/monitor postures, plus a DNS relay; explicitly consumer-agnostic, not LLM-specific). Per ADR-0001, isolation is runner-side per-session policy; the tiers become capabilities a placement advertises.

## The three layers

1. **Egress capture + ACL** — eavs egress firewall. Backend-agnostic, holds no secrets. **Done.** The remaining level-2 work (oqto-h6hr) is the sandbox-side netns + DNAT redirect front-end that captures an agent's traffic and feeds eavs over PROXY v2. The proxy endpoint exists; the capture plumbing does not.
2. **Credential mediation** — placeholder -> real value, substituted only for allowlisted destinations (post-ACL). Explicitly out of scope for layer 1 (`egress.rs` line 18). eavs already implements this for LLM provider keys (virtual key -> real key); oqto-0q7x generalizes it to arbitrary credentials.
3. **SSH** — cannot be SNI/Host-rewritten, so it needs a protocol-level mediator: `oqto-ssh-proxy` (a real 461-line SSH-agent-protocol implementation with allowed_hosts/allowed_keys policy). It is kept and rehomed under the runner (runner spawns it per session, sets `SSH_AUTH_SOCK` to its socket, supplies policy and the approval channel — replacing the legacy direct-to-backend `--oqto-server` HTTP prompt path).

## Pluggable secret provider

Layers 2 and 3 depend on a `SecretProvider` trait, never on a specific vault:

```
trait SecretProvider {
    fn resolve(placeholder, destination, ctx) -> Option<Secret>;  // allowlist-gated
}
```

kyz is one implementation, not the interface; Infisical, HashiCorp Vault, and a trivial env/file provider (for personal installs) are peers. eavs's existing virtual-key store is effectively the first implementation and becomes the reference. The SSH proxy's upstream is therefore "the configured `SecretProvider`," not the ambient `ssh-agent` and not hardcoded kyz.

## Truth in config

A config option that is not enforced by code must hard-error at startup, never parse-and-ignore. `NetworkMode::Proxy` must refuse to start a session until the layer-1 redirect front-end (oqto-h6hr) lands, then becomes real. This is a standing rule (the dead `oqto-runner.socket` unit and the closed-but-unimplemented sandbox-v2 epics were the same pathology); oqto-pgwp becomes "apply truth-in-config + reopen what is actually unbuilt."

## Default isolation tier per profile

- **Team installs: level-2 by default**, explicit per-workspace opt-down. Shared multi-user infrastructure is exactly the exfiltration threat model; default-open is indefensible once the capability exists.
- **Personal installs: open by default**, one-line opt-in to level-2, with egress posture surfaced in the UI. (Softest part of this decision — the counterargument is that prompt-injected agents on a laptop are a real exfiltration vector, which would argue for level-2 default + a visible "egress: open" toggle even personally. Revisit once level-2 is low-friction.)
- **Level-3 microVM: opt-in per session/workspace**, advertised as a placement capability (ADR-0001/0004). Never a baseline requirement, or KVM becomes a setup prerequisite.
