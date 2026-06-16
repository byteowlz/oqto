# Memory is embedded mmry-core, not a proxied service

Oqto previously reached memory through a per-user `mmry` HTTP daemon proxied via `/v1/memories` (`api/proxy/mmry.rs`) — the source of recurring "Failed to fetch memories: Service Unavailable" failures (oqto-wygp). We embed `mmry-core` as a library instead: the runner reads/writes `<workspace>/.mmry/mmry.jsonl` directly (append-only; deletion is deprecation). No per-user mmry service, ports, TOML config, or proxying. Access path becomes frontend -> api -> runner -> `mmry-core`; the `/api/workspace/memories*` endpoints stay, mapped onto mmry-core, so the UI is unchanged. (oqto-6ek1, in progress.)

This is the same embedded-not-service principle behind the control plane (ADR-0003) and is the in-oqto consumer of the broader mmry redesign (lean append-only per-workspace store; embeddings move to vqtrs).

## Consequences

- `api/proxy/mmry.rs` is deleted as part of this cutover (one of the three `proxy/` legs; see ADR-0008 delete-and-collapse — proxy deletion follows its replacements landing).
- Memory access is runner-mediated, consistent with ADR-0009's user-plane dissolution.
- Intentional short-term loss: semantic search, reranking, named/global stores, session dumps. Memory remains workspace-local lexical-only; semantic search is deferred to vqtrs. Memory itself is never lost — only the daemon and the network hop.
- A deploy/update migration converts legacy SQLite stores into `.mmry/mmry.jsonl` idempotently.
