# Memory is embedded mmry-core, not a proxied service

Oqto previously reached memory through a per-user `mmry` HTTP daemon proxied via `/v1/memories` (`api/proxy/mmry.rs`) — the source of recurring "Failed to fetch memories: Service Unavailable" failures (oqto-wygp). We embed `mmry-core` as a library instead: the runner reads/writes `<workspace>/.mmry/mmry.jsonl` directly (append-only; deletion is deprecation). No per-user mmry service, ports, TOML config, or proxying. Access path becomes frontend -> api -> runner -> `mmry-core`; the `/api/workspace/memories*` endpoints stay, mapped onto mmry-core, so the UI is unchanged. (oqto-6ek1, in progress.)

This is the same embedded-not-service principle behind the control plane (ADR-0003) and is the in-oqto consumer of the broader mmry redesign (lean append-only per-workspace store).

## Update (2026-07-23)

mmry itself is now lean: the current source is only `mmry-core` + `mmry-cli`, with no service crate, embeddings, or reranking. `mmry-core` is embedded in the **oqto-runner crate**; the live memory API path is frontend -> api -> runner -> `mmry-core` reading `<workspace>/.mmry/mmry.jsonl`. The backend resolves the workspace's runner (`resolve_runner_for_workspace_path`) and never opens the ledger in-process, so memory works in container mode and respects the runner isolation seam (see ADR-0026, which supersedes this access-path detail). There is no per-user mmry daemon and no central embeddings service anymore. The previously "deferred to vqtrs" embeddings work is out of scope of mmry; semantic search, if reintroduced, is a separate system over the workspace ledger. Removing the dead HTTP subsystem (per-user `mmry.service`, central `mmry-embeddings`/`mmry-central`, `MmryConfig`/`MmryState`, `UserMmryManager`, `mmry_port` allocation, and container `MMRY_HOST_URL` injection) is tracked under oqto-hm11.

## Consequences

- `api/proxy/mmry.rs` is deleted as part of this cutover (one of the three `proxy/` legs; see ADR-0008 delete-and-collapse — proxy deletion follows its replacements landing).
- Memory access is runner-mediated, consistent with ADR-0009's user-plane dissolution.
- Intentional short-term loss: semantic search, reranking, named/global stores, session dumps. Memory remains workspace-local lexical-only. Memory itself is never lost — only the daemon and the network hop.
- A deploy/update migration converts legacy SQLite stores into `.mmry/mmry.jsonl` idempotently.
