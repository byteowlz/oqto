# oqt tracks upstream Pi Durable: single owning host, attached clients, upstream sync

## Status

Accepted by the user, 2026-10-22; `oqto-1b8f`. Supersedes building further on the
pinned experimental client/server stack (ADR-0063/0065 transport assumptions).

## Context

oqt is pinned to Pi `cdaf6c22` (2026-10-11). Upstream removed the experimental
client, server, protocol and coding-agent client/server/worker modules oqt is
built on (commit `7472cda0`). Pi Durable remains maintained. Its README states one
process owns a storage, with no cross-process locking; multiple readers are views
and event streams attached to that process, including late-joining remote
clients. The active upstream `replication` branch adds `pi-durable/sync`: commit
log replay, once-created remote slots per batch, per-device continuation with
explicit divergence, pull/restore and folding. Older client/server branches share
no history with main and are abandoned.

## Decision

- oqt tracks upstream closely. `just test-oqt-upstream-drift` fails when the pin is
  more than 14 days behind or oqt depends on removed upstream paths, and reports
  active upstream branches touching oqt dependencies. Upgrade at least once per
  upstream release.
- Architecture follows upstream's shape: one owning host process per Session's
  storage; clients attach through Durable views/event streams. No multi-writer or
  shared-database design.
- oqt owns only a thin host/transport/worker layer over maintained `pi-durable`,
  `pi-tui` and `pi-env`, keeping guard authority, Session/conversation identities,
  receipts and the ADR-0070 presentation seam. Superseded source patches are
  deleted, not ported.
- Moving Sessions between machines (laptop, Mac Studio) uses upstream
  `pi-durable/sync` once it lands on main; oqt does not build its own replication.
- If upstream ships a Durable client/server, adopt it and delete the owned layer.
- Durable's SQLite schema v3 migration is one-way; pin bumps that migrate storage
  require an explicit catalog decision, never silent migration of real Sessions.
- oqt clients are OqtoUI hosts: ADR-0038's rule applies (no agent- or
  host-supplied code loads into a host process; rich UI arrives as declarative
  data rendered by the host's own registry, or as sandboxed web content), and
  first-party surfaces stay bundled per ADR-0027. Upstream's networked plugins
  ship client code from the server; oqt does not adopt that for its TUI. See
  `docs/research/2026-10-22-fragmented-312-pi-durable.md`.
