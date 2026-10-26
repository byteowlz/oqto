# Personal Session configuration and live observations

Personal Sessions remain terminal-owned. No Pi spawn, reload, native transcript write, new grant or Server history migration is part of this feature.

- `SessionModels` is an observe action. It reads the existing Pi bridge catalog without an input lease. Prefer nonempty session-scoped models; project provider/id/name only, never credentials, headers or provider URLs.
- `SessionRead.details` optionally reports the observed model, thinking level and queue presence/count. Missing queue fields mean unknown, not zero.
- Typed `SetModel`, `SetThinkingLevel` and `FollowUp` inputs use the existing paired control admission and native same-connection lease. Configuration changes require idle Pi and validation against its live advertised options. Existing binding, source, generation and revocation fences remain. No arbitrary RPC, implicit abort or automatic mutation retry.
- Parallel tool observations are keyed by call ID and bounded to 64 active calls. These are lossy previews, not transcript/event replay.
- Session Files opens that Session's cwd directly, independent of Server/personal placement. `SessionFiles` resolves the exact authorized public Session on the host, then maps its cwd into an existing, inode-validated Files grant. Capability-relative directory opening and inode comparison fence the mapping; sibling prefixes, symlink escapes and replaced roots fail. No native attachment or input lease is needed. The phone never derives authority from `Session.workspace`. Existing signed `FilesList`/`FileRead` retain their grant checks.

The deployed bridge already supports model/thinking/follow-up methods, so this rollout does not need a Pi reload. It does not expose full queues. Optional queue observations are forward-compatible; complete queue lists/clear remain unsupported. Abort is not clear-queue.

Tests: native synthetic peers cover secret projection, scoped catalog/no lease, guarded typed inputs and overlapping tools/bounds. Existing control revocation/switch/outcome tests remain required. Mobile signed synthetic integration covers root IDs, nested previews, no attach/lease for browsing and configuration acknowledgements followed by fresh observations. Real provider mutation/generation is not a qualification step.
