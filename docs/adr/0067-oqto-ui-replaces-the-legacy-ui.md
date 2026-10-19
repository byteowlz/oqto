# ADR-0067: OqtoUI replaces the legacy UI, with one admin surface and a browser-local setup agent

## Status

Proposed (2026-10-19). Refines the end state of epic `oqto-a9j4` ("OqtoUI absorbs the
legacy frontend"); parity evidence lives in
[`docs/frontend/oqto-ui-parity-inventory.md`](../frontend/oqto-ui-parity-inventory.md)
(`oqto-a9j4.6`). Builds on ADR-0041 (compositor), ADR-0053 (Selection), ADR-0061
(account sign-in) and ADR-0029 (operator authentication).

## Context

Two web frontends ship today: the legacy UI (default route) and OqtoUI (`/oqto-ui`).
Since August, Desktop (GPUI) and Mobile (native over a shared Rust core) have moved
fast while OqtoUI got four lint/accessibility commits in October. The first parity
pass found OqtoUI missing admin, account settings, tool permission dialogs, message
search, uploads and several workspace panes, with sidebar Settings/Admin/Logout
buttons not wired at all.

The goal is not parity but **surpassing** the legacy UI in quality, look and feel,
and UX **without disorienting** people who know it. Admin is also scattered: legacy
`apps/admin`, `oqtoctl`, and setup scripts each expose different slices.

## Decision

### 1. A measurable bar, not a feeling

OqtoUI becomes the default only when every row of the parity inventory is **better**,
**same**, or **drop** with a written reason. Each green row has a screenshot pair at
the Design Studio desktop, tablet and phone views plus a behaviour check. Cross-cutting
quality gates per screen:

- Design Studio line target met for the view (no target means no visual decision).
- Keyboard-complete and screen-reader labelled; no unwired controls (lint: every
  interactive element has a handler or is disabled with a reason).
- No layout shift on reload, reconnect or history paging; long sessions stay
  responsive (budget recorded per screen).
- Live data identity per AGENTS.md: oqto-log is authority, no reconciliation by
  text/index/order.

### 2. Familiar first, better inside

People keep their bearings: same domain names (`CONTEXT.md`), same location for the
primary actions (new session, model, stop, send, files, settings, sign out), same
default keyboard shortcuts, same session list order. Improvements happen inside those
anchors (compositor layouts, step collapse, paging, reconnect), not by moving them.
Any deliberate move of a primary action needs a row in the inventory with the reason.

### 3. One admin and setup contract, three presentations

Admin and setup operations are defined once in the backend as semantic operations
(users and invites, devices and sessions, sign-in/MFA policy, runners and machines,
models and EAVS, apps, health and deploy state, audit log). They are presented:

- in OqtoUI, as the single admin console (legacy `apps/admin` is retired with it);
- in `oqtoctl`, with the same operations and authorization;
- optionally as WebMCP tools (§5).

Desktop and Mobile expose only user-scoped slices (own devices, approvals,
attention), never a second console. Authorization is enforced server-side per
operation; presentations never decide authority. ADR-0061 sign-in/MFA is the first
slice (MFA policy, factor reset, device revocation).

### 4. Pi Durable in the browser: placement, not authority

Pi Durable may run in the browser tab as an **agent placement** for work that has no
server sandbox yet or needs none:

- **First-time setup agent.** Before any runner, sandbox or provider exists, a
  browser-local Durable agent guides setup: inspects the host through setup
  operations, explains choices, fills configuration, runs and verifies steps. It
  resumes after reload from its own checkpoints. Model access is a user-supplied key
  or local/EAVS endpoint, held in the tab unless the user saves it as the first
  provider.
- **Oqto Flash** (`oqto-7rq1`) and the iOS embedded engine use the same engine
  contract (board: embedded engine as an oq host), not per-platform agent loops.

Constraints:

- The agent has no shell. It calls only §3 operations, each previewed and confirmed;
  security-sensitive steps (first admin, MFA enrolment) are performed by the human.
- Its store is a setup/agent log, never a second chat transcript; it does not write
  runner sessions or oqto-log.
- Not used for UI state or caching (that is the disposable IndexedDB mirror,
  `oqto-a9j4.7`).
- Setup works fully without a model as a plain wizard over the same operations.

### 5. WebMCP, optional

When the browser supports WebMCP and the user enables it, OqtoUI registers §3
operations (and selected session actions) as WebMCP tools, generated from the same
operation schemas. Tools act with the user's session and permissions only; writes
require confirmation; sensitive operations still require MFA step-up (ADR-0061);
every call is audited. Prior art: `oqto-website-webmcp`.

### 6. Staged cutover

1. Fix blocking findings (dead controls, identity, sign-out).
2. Grind inventory rows; legacy is feature-frozen (bug fixes only).
3. **Opt-in:** per-user switch to OqtoUI with a visible "back to the classic UI".
4. **Default:** OqtoUI default; switch-back remains for one release.
5. **Removal:** legacy tree deleted once the inventory is green and switch-back
   usage is negligible.

## Consequences

- One admin vocabulary across Web, CLI and agents; no feature lives only in a
  legacy panel or only in a script.
- Setup becomes agent-assisted without weakening the authority model: the agent is
  a client of the same operations as a human.
- Cost: the admin/setup operation layer must exist before the console; several legacy
  panels (dashboard, agents catalog, sldr) need an explicit port-or-drop decision.
- Risk: Pi Durable in the browser is experimental (API may change); §4 starts with a
  spike that proves reload-resume and two real setup operations before adoption.

## Open questions

- Does the Rust compositor (wasm) become the single layout kernel for Web and
  Desktop before large layout work? (board proposal 2026-10-04; needs its own ADR)
- Which legacy panels are dropped rather than ported (dashboard, agents catalog,
  sldr, canvas)?
- Where does the setup operation layer live: `oqto-setup`, the backend, or both
  behind one schema?
