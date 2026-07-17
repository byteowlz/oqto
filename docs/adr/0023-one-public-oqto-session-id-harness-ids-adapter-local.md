# One public Oqto session id; harness ids are adapter-local bindings

Status: proposed (2026-07-03). Companion to ADR-0013 and prerequisite for the Work Session read model / attach contract ADR.

Oqto has previously been hurt by multiple competing session ids: frontend-generated generic ids, backend platform ids, and harness-native ids from pi or other agents. That ambiguity caused fragile joins, reconnect bugs, and message reconciliation mistakes. As Oqto expands from pi to Claude Code, Codex, OpenCode, Hermes, and app-server-backed integrations, the identity rule must be explicit and mechanically enforceable.

## Decision

Use exactly one public session identity across Oqto: the **Oqto session id**.

- The Oqto session id is the only id used by public surfaces: frontend routes, CLI / `oqtoctl`, backend REST/WS APIs, `oqto-log` keys, gvnr/govnr references, and human-facing links.
- The backend session authority mints the Oqto session id once. It is not re-minted across reconnect, resume, harness restart, process restart, or placement move.
- Forks create a new Oqto session id and record lineage with `parent_id`; they do not reuse the parent id.
- Harness/runtime ids are adapter-local bindings: pi JSONL session id, Claude Code session id, Codex thread id, ACP handle, tmux pane id, pid, PTY id, and similar values. They may be stored as external binding facts, but never become route params, public ids, or cross-component join keys.
- After a runtime binding exists, every persisted record and session event must carry `oqto_session_id`. External ids may be included as annotations, but are not sufficient for joining.
- Oqto-session-to-external-id mappings are append-only, auditable facts with provenance (`kind`, `external_id`, `source`, `first_seen`, optional `last_seen`). Rebinding appends a new fact rather than mutating identity in place.
- `pending-*`, `tmp:*`, optimistic frontend ids, and other provisional ids must not cross persistence or public API boundaries. Components must fail closed or wait until the backend has minted the real Oqto session id.
- Attach targets are capabilities a session has, not identity. A tmux pane, PTY, pid, SSH target, or container exec target is addressed through the Oqto session id.

`AGENT_CTX_PLATFORM_SESSION_ID` is the process environment representation of the Oqto session id. Harness-specific env values, when present, stay under harness-owned keys such as `AGENT_CTX_HARNESS_SESSION_ID`.

## Fit with existing ADRs

- ADR-0003 and ADR-0013 already establish platform session ids and `AGENT_CTX_*` as join keys for the control/awareness plane.
- ADR-0006 makes bridge translators the natural owners of harness-native ids and capability advertisement.
- `oqto_log_sessions.platform_id` plus `external_id` is the reference storage shape: platform id is Oqto identity; external id is a binding fact.

## Consequences

Frontend code never mints durable session identity. Adapters must translate harness-native identity into binding facts. Reconciliation by text, index, array length, visible order, pid, or pane id remains banned.

Add mechanical checks over time:

- lint or tests rejecting `pending-*` / `tmp:*` ids in persisted stores and API payloads;
- doctor checks that bound session events carry `oqto_session_id`;
- API tests proving external ids are not accepted as primary route ids.
