# oq-permissions (experimental, unpublished)

Host-neutral permission authority for the adaptive sandbox prototype. Separate from `oq`/`oqt` presentation and the existing OS confinement implementation.

- `Policy` supplies a pinned baseline and requestable ceiling; unknown targets are denied.
- `Authority` owns a private SQLite permission ledger for one verified binding/audience/policy. Pending decisions survive reopen, approvals bind exact operation digests/revisions, Session grants can resolve pending sibling operations, and revoked/expired grants fail closed.
- `Permit` cannot be deserialized from a client message. Check it with the actual operation before invoking a provider and before delivering results/pages. Any authority generation change conservatively invalidates old runtime permits.
- `ExecutorFence` is admission fencing only. The owning runner must actually drain/cancel/confirm stop and prepare confinement before installing a new generation. The crate does not manage processes or assert that a shell survives restart.

Identity inputs are **trusted runner inputs**, not authentication proofs. The real carrier must authenticate its Account/operator, resolve Principal/Workspace/Session/resource authority and keep approval APIs outside model/tool authority. Never expose `decide` on an agent's tool connection. The fixture demonstrates separate pipes, not production Account authentication.

Baseline policy is immutable for this prototype. Approved overlays adapt live; changing preset/ceiling/audience requires a separately implemented and qualified policy transition. Reopening with changed policy/binding is refused, even if its revision string is unchanged. The ledger is not an existing Oqto data-store bypass, memory/history authority or Chat timeline. Domain providers retain their own authorization/store interfaces.

See [source-only runtime proof and explicit gaps](../../../tools/oqt/README.md) and [ADR-0059](../../../docs/adr/0059-adaptive-capability-gates-and-replaceable-executors.md).
