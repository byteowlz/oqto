# Backend Refactor PR Slicing Plan (Epic oqto-36zw)

Date: 2026-03-17
Related:
- Epic: `oqto-36zw`
- Target tree: `docs/architecture/backend-target-tree.md`

## Objectives

- Ship the runner-first refactor incrementally with low regression risk.
- Keep protocol behavior stable while improving structure.
- Ensure each PR is independently reviewable, testable, and reversible.

## PR Sequence

### PR1 — Baseline hardening (`oqto-36zw.1`)

**Scope**
- Expand integration coverage for session/chat/files/memory flows.
- Add path telemetry (`runner` vs `direct`) to observe cutover impact.
- Add deterministic smoke checks for runner path.

**Blast radius**: Medium (test harness + observability only)

**Required gates**
- New integration suite passing in CI.
- Existing backend tests passing.
- Smoke script documented and executable.

---

### PR2 — Runner default path (`oqto-36zw.2`)

**Scope**
- Make `RunnerUserPlane` default in normal local/single-user modes.
- Keep direct path only behind explicit `OQTO_UNSAFE_DIRECT=1`.
- Emit warning when unsafe direct mode is enabled.

**Blast radius**: Medium-High (runtime path selection)

**Required gates**
- Selection behavior tests (default runner, opt-in direct).
- API/WS regression suite green.
- Telemetry confirms expected path usage.

---

### PR3 — Runner resiliency (`oqto-36zw.3`)

**Scope**
- Auto-start runner when socket missing.
- Health/readiness checks before serving runner-dependent operations.
- Retry/backoff and reconnect handling on runner failures.

**Blast radius**: High (startup + recovery path)

**Required gates**
- Recovery tests for runner unavailable and restart scenarios.
- Bounded retries and clear logs/messages validated.
- No hang/deadlock behavior in degraded conditions.

---

### PR4 — Split websocket monolith (`oqto-36zw.4`)

**Scope**
- Split `api/ws_multiplexed.rs` into channel-scoped modules.
- Preserve wire format and channel semantics.

**Blast radius**: High (real-time protocol handling)

**Required gates**
- WS protocol snapshots unchanged.
- Event ordering tests pass.
- No new giant replacement file.

---

### PR5 — Runner daemon modularization (`oqto-36zw.5`)

**Scope**
- Move logic from `src/bin/oqto-runner.rs` into `runner/daemon/*` modules.
- Keep binary as thin bootstrap.

**Blast radius**: High (runner internals)

**Required gates**
- Runner protocol parity tests.
- Process/file/session/pi handler tests pass.
- No protocol/API changes.

---

### PR6 — Extract `oqto-runner` crate (`oqto-36zw.6`)

**Scope**
- Create dedicated runner crate and move runner code.
- Preserve server-runner contract.

**Blast radius**: High (workspace crate boundaries)

**Required gates**
- Workspace builds in all profiles.
- Runner boots and handles protocol calls.
- No cyclic deps introduced.

---

### PR7 — Extract `oqto-sandbox` crate (`oqto-36zw.7`)

**Scope**
- Move sandbox config/policy/platform implementation to dedicated crate.
- Keep CLI wrapper thin and behavior equivalent.

**Blast radius**: Medium-High (sandbox integration)

**Required gates**
- Linux bwrap behavior parity tests.
- macOS seatbelt path compiles/tests where applicable.
- Runner sandboxed process path unchanged.

---

### PR8 — Consolidate history surface (`oqto-36zw.8`)

**Scope**
- Resolve `history/` vs `hstry/` ownership into one clear integration surface.
- Remove duplicate adapters/re-exports.

**Blast radius**: Medium-High (message persistence/read path)

**Required gates**
- Message write/read parity tests.
- Session list/render tests unchanged.
- Existing data compatibility maintained.

---

### PR9 — Unify canonical types (`oqto-36zw.9`)

**Scope**
- Ensure canonical protocol types live in `oqto-protocol` only.
- Remove duplicated local `canon` definitions.

**Blast radius**: High (cross-cutting type usage)

**Required gates**
- Serialization compatibility tests pass.
- No duplicate canonical type definitions remain.
- Protocol contract tests green.

---

### PR10 — Final cutover cleanup (`oqto-36zw.10`)

**Scope**
- Remove production direct runtime path and obsolete flags.
- Update architecture and operator docs.

**Blast radius**: Medium (cleanup after stabilization)

**Required gates**
- Runner-only path in production code.
- Full regression suite green.
- Docs match runtime behavior.

---

## Global Guardrails

- No protocol changes unless explicitly scoped in issue.
- Keep risky behavior behind temporary flag until validated.
- Prefer move-only refactors before behavior changes.
- Require explicit acceptance-criteria validation in each PR description.

## Recommended Merge Policy

- PRs 1–3 sequential (stability baseline + runtime switch)
- PRs 4,5,7,8,9 parallelizable after PR3 with ownership coordination
- PR6 after PR5
- PR10 last

## Execution Workflow (trx)

Use this flow when picking up work from the epic:

1. Check unblocked tasks:
   - `trx ready`
2. Inspect the issue and checklist subtask:
   - `trx show oqto-36zw.X`
   - `trx show oqto-36zw.X.1`
3. Set status before starting:
   - `trx update oqto-36zw.X --status in_progress`
   - `trx update oqto-36zw.X.1 --status in_progress`
4. Implement and validate required gates listed in this plan.
5. Mark checklist complete in issue notes/description updates if needed.
6. Close subtask and parent when acceptance criteria are met:
   - `trx close oqto-36zw.X.1 -r "Checklist complete"`
   - `trx close oqto-36zw.X -r "Acceptance criteria met"`
7. Sync tracker changes:
   - `trx sync`

### Quick navigation commands

- Show epic: `trx show oqto-36zw`
- Show all issues: `trx list`
- Show dependency tree (when implemented): `trx dep tree oqto-36zw`

### Assigned checklist subtasks

- `oqto-36zw.1.1` baseline hardening + telemetry
- `oqto-36zw.2.1` runner-default selection
- `oqto-36zw.3.1` runner bootstrap/recovery
- `oqto-36zw.4.1` websocket channel split
- `oqto-36zw.5.1` runner daemon modularization
- `oqto-36zw.6.1` runner crate extraction
- `oqto-36zw.7.1` sandbox crate extraction
- `oqto-36zw.8.1` history/hstry consolidation
- `oqto-36zw.9.1` canonical type unification
- `oqto-36zw.10.1` final runner-only cleanup
