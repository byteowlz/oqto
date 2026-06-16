# The control plane is a standalone project, not an oqto subsystem

Oqto needs an authority for fleet state — which runners exist, what agents run where, health, capabilities (the registry implied by ADR-0001's dial-in registration). We decided to build it as an independent project from day one rather than an oqto crate extracted later, because it has two foreign client types immediately (runners register/heartbeat; oqto queries/watches) and the owner's broader vision is one control plane across all byteowlz agent surfaces, not just oqto. This passes the test that killed hstry and mmry-as-service (single same-language client on a hot path) — the control plane is slow-path, multi-client, and holds no product data.

## Anti-drift guardrails (conditions of this decision)

1. **Charter**: the repo carries an IS/NOT scope list before code. IS: who, where, alive, capabilities, placement, health. NOT: chat content, history, workspace data, user-facing product metadata.
2. **Wire protocol as the only interface**, even when embedded — no shared DB, no rich-type backdoor.
3. **Core-lib + thin server** (mmry-core pattern): oqto embeds the core on personal installs (no extra daemon), connects to the server in fleet mode.
4. **One conformance suite, two targets**: the same tests run against embedded core and server binary in the project's CI; oqto pins the protocol crate version.
5. **Data-plane fence**: message/event streams never route through the control plane; it answers "where is X," then clients talk to runners directly.

## Consequences

- The `oqto-3ct7.8` session-orchestration extraction is the consumer-side split: product session metadata stays in oqto; placement/registry logic targets the control-plane contract. The protocol should be defined before that extraction lands.
- `AGENT_CTX_*` (schemas/agent-context-env) provides the shared join keys (platform/harness session ids, workspace id, user id) between registry records, oqto history, and tool telemetry.
