# Backend decomposition: delete-and-collapse before extract; always through the runner

The backend crate refactor program (oqto-3ct7) was written before ADRs 0001-0007 and would, if followed as numbered, extract dead and misplaced code into fresh crates. We adopt a hard ordering rule: the first decomposition PRs are subtractive. No domain crate is carved out until the code it would contain is correct.

## The rule

Before any extraction touching sessions, history, or execution:

1. **Delete the `container` and `local` modules** (~2.4K LOC) unconditionally. Execution always goes through a runner — including single-user/personal/dev deployments. There is no direct-spawn path to preserve. (Reaffirms ADR-0001; personal runner lifecycle per ADR-0002 — a backend-supervised same-uid child runner where systemd is absent.)
2. **Collapse `session`'s runtime-mode branching** before extracting `oqto-sessions` (3ct7.8). Extracting first would crystallize the `match runtime_mode` complexity we are abolishing into a new crate.
3. **Delete `canon/from_hstry.rs`** (ADR-0005) before/with the `oqto-history` extraction (3ct7.9), so the new history crate never learns hstry's proto shape.

## Extraction order (leaves -> root)

- `oqto-history` (3ct7.9, in progress) proceeds independently — runner depends on it, no api dependency.
- Domain crates (`oqto-users`, `oqto-workspaces`, `oqto-auth`) next; handlers already import these modules, so the seams exist.
- `oqto-sessions` (3ct7.8) only after the runtime-mode collapse AND after the gvnr protocol crate exists, because the control-plane boundary (ADR-0003/0004) cuts through it: product session metadata stays in oqto-sessions; placement/registry/routing target the gvnr contract (the `session_target` + `runner` modules are that seam).
- `oqto-api` (3ct7.10) last among extractions — it is the dependency sink (20K LOC: REST handlers + WS-multiplex gateway + proxy).
- `oqto` slims to a composition root (3ct7.11): config load, service construction, route mounting, server start — no domain logic. This is the success test.

## Consequences

- The first PRs show deletions, not new crates. This is correct progress, not slow progress.
- `local`-mode deletion is gated only by the runner-personal path being verified (ADR-0002), not by any design question — the direction is settled.
- `oqto-sessions` extraction is explicitly blocked on the gvnr protocol; do not finalize it before then.
