# Layout test-drives: Web parity and shared contracts (coordination brief)

Status: intake, 2026-10-04. Coordination for govnr epic `gvnr-t4z4`; full user
request and proof obligations live in the govnr brief
`govnr/docs/briefs/oqto-layout-testdrives-and-project-spaces.md`. This brief
accepts no ADR, picks no winning layout and starts no backend redesign.

## Scope (user decision 2026-10-04)

- **oqto-desktop is the first implementation and test-drive host.** Its
  adapter intake (navigation presets, integrated sidebar, local recorder,
  repeated test-drive pilot) is filed in the oqto-desktop repo, not here.
- **Oqto owns the shared contracts** the desktop pilot must not invent alone,
  and **Web adopts them as they are proved**: the navigation configuration,
  the project-grouping semantics, and the interaction measurement schema.
- **Domain questions stay proposals.** The project-grouping vocabulary is not
  accepted vocabulary and is not a dependency of the desktop pilot.
- Priority: next, after the current Desktop composer/preview integration.
  Workflows are intended for the coming days; no date or time was set.

## Where things stand

- OqtoUI (web) lives on `feat/oqto-ui-chat-renderer-parity`
  (`frontend/src/oqto-ui/`), not on `main`. Issues filed from `main` reference
  OqtoUI issues on that branch by id (`oqto-a9j4`, `oqto-nq3b`, `oqto-r5yn`,
  `oqto-fv7s`); trx dependencies across the two stores cannot be linked until
  they merge.
- Accepted: ADR-0037 (portable Views, Screen Modes), ADR-0041/0042
  (compositor, grid), ADR-0043 (Arrangements, binding, active Workspace, the
  switcher rail as an ADR-0040 Slot).
- Proposed, not accepted: ADR-0040 (Customizations: Slots, Providers, Actions,
  presets as data), ADR-0050 (layout recipes: geometry of side panels, strips,
  primary columns). ADR-0050 covers geometry only, not the whole navigation
  contract.
- Evidence so far: design-studio `bb156c6`, `artifacts/oqto/context-pane/`:
  gallery A–E with 166 browser checks and synthetic travel metrics. Those are
  not human performance evidence.

## Boundaries every child issue keeps

1. **Chrome is not Content.** The project rail and Sessions list may share one
   visual frame (integrated sidebar), but persistent switcher chrome stays a
   Slot (ADR-0043/0040), not closable Container Content. Configuration
   switching changes presentation only, never ownership or authority.
2. **One substrate, declarative presets.** A–E and the integrated sidebar are
   data over one model, not per-layout code paths. Unsupported combinations are
   diagnosed, never silently coerced.
3. **Grouping grants nothing.** A user-defined collection moves no data,
   mounts nothing, injects no context, selects no execution target.
   Workspace (isolation/tenancy) and Arrangement (layout) keep their meanings.
4. **Measurements are private and judgement-free.** Opt-in, local, explicit
   export/delete/retention, no content or identifying strings, no automatic
   upload, no automatic layout winner or productivity score.

## Decisions still open (need the user or an ADR)

- Whether the navigation configuration contract becomes an ADR-0040 amendment,
  an ADR-0050 extension, or a new ADR once Desktop has proved it.
- Whether project grouping is a saved collection over existing references
  (Arrangement binding plus a Provider filter) or a new durable entity; its
  final name; whether sharing is ever in scope.
- Where the shared measurement schema's canonical definition lives (Rust crate
  with generated TypeScript, or a JSON Schema both hosts vendor) and the
  measured overhead budget.
