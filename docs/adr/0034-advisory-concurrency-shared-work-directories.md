# Advisory concurrency for shared work directories

## Status

Accepted (2026-08-04). Tracked by `oqto-3cw1`. Builds on Session attribution in [ADR-0033](0033-session-file-attribution-and-supersession.md), the expected-version save contract in [ADR-0032](0032-frontend-state-ownership-and-workbench-stack.md), harness capability honesty in [ADR-0006](0006-pi-first-class-harness-others-bridged.md), and the one-runner-per-Workspace placement in [ADR-0019](0019-container-per-workspace-placement.md).

## Context

Many Sessions share one work directory filesystem, and concurrent Sessions can edit the same files. The shared tree is a product premise — [ADR-0031](0031-session-centric-workbench-shell.md) makes the filesystem the common human/agent frame of reference — and the primary users include people for whom "your files are now in three worktrees" is a worse outcome than an occasional conflict. Files must appear where they are expected to be.

Two mechanical protections already exist and are easy to overlook:

- **Agent side:** content-anchored edit tools. Pi's edit tool applies a change only where the exact prior content still matches; if another writer changed the file, the edit fails, the agent re-reads and rebases. This is compare-and-swap enforced at the last moment, with the retry loop delegated to the model. Full-file writes and shell redirects are not covered by this anchor; harness staleness checks mitigate but do not guarantee.
- **Human side:** ADR-0032's expected-version saves protect a person's editor buffer from an agent writing underneath it. No harness tool can provide this; it is ours.

The judgment/mechanism cut drives the rest: conflict *detection*, versioning, and messaging are deterministic mechanism; deciding *who should proceed* and *how to reconcile* is judgment that models exercise better every generation, and that a platform must therefore expose rather than encode.

## Decision

### 1. Coordination is advisory: transparency plus Session-addressed notification

Concurrent Sessions coordinate through information, not enforcement:

- **Transparency:** File Activity (ADR-0033) shows which Session claims which paths, and unattributed work-directory changes, in the live tree.
- **Notification:** a Session may notify another Session with a durable message addressed to it, carried through the runner on the canonical protocol. It has a named sender, a named recipient, and it persists in the recipient's Session — so the coordination is visible to the human in that Session's chat, not hidden machine traffic.

This is not an event bus: nothing is anonymous, nothing is fire-and-forget, and no new authority is created. It composes from primitives that already exist.

### 2. The floors stay where they are; we do not rebuild them

The agent-side floor is the harness's content-anchored edit behavior. It is per-harness, and per ADR-0006 that variance is honest, queryable capability — not a defect to fix with a platform layer. The human-side floor is the ADR-0032 expected-version save.

**Explicit non-goal:** no platform-level file locking, lease, or duplicate compare-and-swap layer. Pi already rejects stale edits; a second mechanism guarding the same invariant would create a competing authority and add the coordination policy we are deliberately not encoding.

### 3. No enforcement layer

Nothing blocks a Session because another Session claimed a path. No lock manager, no lease, no scheduler assigning files to agents. The advisory signal informs; acting on it is the model's judgment, and resolving a genuine conflict belongs to the model or the human with both diffs and both intents in hand.

Advisory coordination has an inherent race — two Sessions can check, both see clear, both proceed. The floors turn that from silent loss into a rejected write and a rebase. Collisions become rare and non-destructive, not impossible: two individually-valid edits can still be jointly wrong, and no mechanism detects semantic conflict. That wall is irreducible; the substrate's whole duty is to surface both sides to a resolver.

### 4. The escape hatch: opt-in isolated workspace mode

Advisory coordination has a realistic ceiling. A handful of concurrent Sessions in one tree coordinate well; a fleet does not, and remote runners make fleets the expected shape eventually.

For that regime, a Session may be started in **isolated workspace mode**: its work happens in a copy-on-write snapshot (btrfs/reflink/APFS clone, or a Git worktree where a repository exists) whose parent is recorded, generalizing ADR-0033's base anchoring from "Git commit or content hash" to "snapshot parent." Changesets, attribution, and supersession fall out structurally; merging back is an explicit, delegated act with full context — never automatic.

Two constraints keep the hatch from swallowing the default:

- **It is a lever, not a heuristic.** Isolation is chosen when the Session is started — by the human or by a delegating agent. The platform never auto-detects "these agents will collide" and silently isolates; that is exactly the encoded coordination policy this ADR rejects.
- **The shared tree remains the default** because non-technical legibility is a product constraint. Isolated mode is expected to be chosen mainly for delegated bulk work on remote runners, where nobody is watching the tree live.

## Rejected alternatives

- **Worktrees/CoW isolation as the default.** Structurally clean and attribution-friendly, but files stop being where people expect them, and every task inherits a merge. Wrong default for a product whose premise is one shared frame of reference; retained only as the opt-in hatch.
- **CRDTs.** Deterministic and convergent, but convergence is not correctness: for code, per-character last-writer-wins is an arbitrary resolution policy laundered as truth, producing syntactically valid, semantically wrong files while deleting the conflict signal the resolver needed.
- **Platform file locking or leases.** Encodes a scheduling policy, serializes the parallelism agents are used for, and duplicates an invariant the harness edit tools already hold.
- **Semantic merge heuristics in the core.** Judgment in mechanism; a stronger model does this better from raw diffs, and the heuristic becomes permanent dead weight.
- **A coordinator that assigns files to agents.** The most model-fragile option: hand-engineered coordination that a better planner routes around.

## Consequences

- Concurrency safety is layered: rare (advisory coordination) × non-destructive (content-anchored floors) × resolvable (both intents surfaced). No layer pretends to eliminate conflicts.
- Notification lands in Session chat, so agent-to-agent coordination is human-visible by construction.
- Harness variance in edit anchoring remains visible capability truth; bridged harnesses without it get the same advisory signals and weaker floors, not a compensating platform lock.
- Isolated workspace mode adds a merge obligation wherever it is used, and requires the placement's filesystem to support cheap snapshots (or Git); that requirement is advertised as a placement capability rather than assumed.
- The parallelism ceiling of the shared tree is accepted and documented rather than engineered away.
