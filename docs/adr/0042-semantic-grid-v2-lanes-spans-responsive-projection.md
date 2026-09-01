# Semantic grid v2: lanes, spans, and deterministic responsive projection

## Status

Accepted (2026-09-01). Tracked by `oqto-a9j4.11`. Refines the grid-strategy internals and mobile-projection decisions of [ADR-0041](0041-oqto-ui-container-compositor-and-agent-control.md); companions are [ADR-0037](0037-oqto-ui-portable-rearrangeable-views.md) (Screen Modes, device-local persistence), [ADR-0040](0040-oqto-customizations-lua-configurable-ui.md) (Slots, which remain distinct from lanes), and [ADR-0043](0043-arrangements-scrolling-binding-overview.md) (Arrangements, Workspace switching, overview; this ADR describes the grid within one Arrangement). It changes no ADR-0041 decision outside the grid strategy: Containers, Container Content, the transaction seam, guardrails, and Agent-control contracts are untouched.

## Context

The first compositor kernel implements ADR-0041's grid as columns of stacked cells. That subset renders the classic preset but cannot express four required layouts:

1. a full-width horizontal strip of Containers — for example a slim terminal band along the bottom, or an equivalent strip at the top;
2. a full-height sidebar coexisting with such strips (the sidebar spans them, or stops at them, per preset);
3. midsize behavior in which an auxiliary Container's tabs merge into the primary Container's tab strip when the inline axis gets tight, and un-merges when space returns; and
4. layouts logically wider than the screen, with Containers kept off-screen and scrolled into view compositor-style (Niri-like), which requires viewport visibility to become explicit model state rather than an implicit topology-equals-screen assumption.

ADR-0041 already names the intended model — "a semantic grid with versioned logical tracks, placements, spans, and constraints such as full height" — and already requires that when a viewport cannot satisfy declared minima, "the profile's deterministic overflow/collapse priority applies and the solver reports degradation." This ADR specifies both, and fixes the responsive ladder between the desktop grid and ADR-0041's mobile projection.

ADR-0037 constraints carried forward: Screen Modes (ADR-0037's Layout Profiles, renamed) never overwrite one another, and resizing must never destroy a layout a user made.

## Decision

### The grid becomes tracks, placements, and spans; a Lane is a row track

Layout `schemaVersion` 2 replaces columns-of-cells with a true two-dimensional semantic grid:

- ordered **row tracks** and **column tracks**, each with an id and a semantic `TrackSize` (fraction or fixed logical length, optional minimum);
- **placements** assigning each Container exactly one rectangle of tracks: row, column, rowSpan, colSpan.

A **Lane** is the domain name for a row track used as a horizontal strip of Containers — one slim Container at the bottom, or several side by side, at the top or bottom of the Arrangement. Lanes are ordinary tracks, not a new object kind. A full-height sidebar is a placement with `rowSpan` covering all rows; a preset may instead stop it above a bottom lane by spanning fewer rows. The classic preset is unchanged in appearance: one row, three columns.

Semantic commands are unchanged in vocabulary. `split` at an inline edge inserts a column track; `split` at a block edge inserts a row track scoped to the target's columns. Dragging Content to the top or bottom viewport edge creates a Lane: a new first or last row track with the Content's new Container spanning all columns, `remove` empty behavior. Dragging to the left or right edge creates a full-height column, as today. Track repair on removal, invariant checks (every Container exactly one placement, no overlapping placements, no empty tracks, spans within bounds), structural sharing, revisioned atomic transactions, and the guardrail gates all carry over.

### Lanes are horizontally locked; an Arrangement scrolls horizontally

All lanes share the global column tracks: the grid is one aligned surface, not a set of independently scrolling strips. The cost is accepted openly — resizing a column moves every lane's boundary at that track — and is softened in practice because slim lanes usually hold Containers spanning several columns. A per-lane independent scroll mode is explicitly **deferred**: it becomes a lane-level property only when a real preset needs it, per the no-speculative-modes rule. This deliberately evolves the one grid strategy rather than opening ADR-0041's "scrolling columns" second-strategy seam, which would fork tab/split/move semantics.

An Arrangement's logical inline extent may exceed the screen; Containers can sit off-screen and are scrolled into view. The **scroll anchor** is the model's viewport-visibility state (the axis ADR-0041 already keeps separate from topology and focus): a semantic resting position — an anchor column track plus the visible span — never a pixel offset. Discrete semantic commands move it: `scroll` by column, and `reveal`, which gains its full meaning — revealing off-screen Content scrolls it into view. In-flight scroll animation is renderer state; only the settled anchor is canonical, so continuous gestures never generate per-frame transactions, and Agent reveal/focus disruption limits stay meaningful. The scroll anchor persists with the Screen Mode's layout document: reopening restores where the user left off. Scrolling is horizontal only; the block axis always fits the screen.

### Flush is a lane-level property

A Lane may be declared **flush** (sticky). Only the first or last row track may be flush. A flush Lane is excluded from the gap rhythm, pinned to its viewport edge while respecting host safe areas, and keeps its declared block size; the web adapter renders it with sticky positioning, native hosts with their equivalents. A flush Lane is additionally **scroll-pinned**: it stays fixed at its screen edge while the normal lanes scroll beneath it, so a sticky bottom strip remains visible regardless of horizontal scroll position. Flush is not available per Container — a single sticky Container inside a normal Lane is rejected as configuration, which keeps the trust boundary clean:

> A Lane hosts Container Content and is layout state. Status segments, corner slots, and other chrome remain ADR-0040 Slots and never become Lanes.

### Responsive behavior is a deterministic, non-destructive projection

Viewport changes never mutate the canonical layout document. The kernel gains a pure projection function: given the canonical snapshot and a viewport class, it returns a projected topology for rendering, with provenance recording every merge it performed. Narrow-then-widen restores the canonical layout exactly, by construction.

The overflow ladder, applied deterministically when declared minima cannot be satisfied on the inline axis:

1. **Collapse before merge:** the navigation-role Container collapses to its rail/drawer presentation first.
2. **Merge inline neighbors into tabs:** within each row, Containers merge into their merge target's tabs in priority order — by default the auxiliary role merges into primary first, then remaining neighbors by declared order. Merge priority is role-based data in the Screen Mode; a per-container override is deferred until a preset needs it.
3. **Lanes persist longer:** merging is per-row, so a slim bottom or top Lane survives widths at which side-by-side columns cannot; a flush Lane persists into the mobile Screen Mode as a bar or sheet.
4. **Mobile Screen Mode:** below the mode threshold the host switches to ADR-0041's mobile projection — one focused destination holding the merged tabs, navigation as a drawer, flush Lanes as bars/sheets. Mobile remains a separate, separately persisted Screen Mode document.

While a merge projection is active, `reveal`/`activate`/`focus` address Content ids and resolve through the projection; topology-mutating commands (`move`, `split`, `resize`) apply to the canonical document. Events and solved geometry report what was actually satisfied, including merge provenance, honoring ADR-0041's host-reports contract. The old solver behavior — scaling below minima with a degradation report — remains only as the terminal fallback when the ladder is exhausted.

### Migration and conformance

- `schemaVersion` 2 ships with a deterministic v1→v2 migration (each v1 column becomes a column track; its cells become row placements; existing sizes carry over) through the existing migration registry.
- The conformance corpus under `contracts/oqto-ui/traces/` is regenerated in the same change: transition traces for lane creation/removal, span invariants, and flush geometry; projection fixtures proving the merge ladder and its exact reversal; migration golden documents.
- Unknown-variant preservation, last-known-good recovery, kernel guardrails, and the public seam are unchanged.

## Consequences

- The solver becomes genuinely two-dimensional (spans, per-axis minima); this is the cost of expressing lanes and full-height spans honestly instead of nesting ad hoc levels.
- Midsize stops being degradation and becomes designed behavior; users get tab-merging for free from the same tab mechanics the compositor already renders.
- The projection function is pure kernel code, so GPUI or other hosts inherit identical responsive behavior through the trace corpus.
- Presets and future Customizations (ADR-0040) can declare lanes, flush edges, and merge priorities as plain data.

## Rejected alternatives

- **Mutation-based merging** (issuing `move` commands when the window narrows): destroys user layouts, breaks Screen Mode isolation, and makes widening lossy.
- **A third nesting level** (lanes → columns → cells): cannot express a full-height sidebar beside lanes and is a disguised split tree, which ADR-0041 already rejected.
- **Per-container sticky:** overlaps ADR-0040 Slot chrome and invites floating-panel semantics the compositor does not want.
- **Viewport-only CSS reflow:** invisible to native hosts, non-deterministic across renderers, and unrepresentable in the conformance corpus.
- **Keeping the v1 topology and special-casing a "bottom bar":** every future strip (top bar, second bottom lane) would need another special case; tracks-and-spans is the model ADR-0041 already committed to.

## Verification

1. Property tests over generated v2 command sequences preserve the extended invariants: single placement per Container, no overlaps, no empty tracks, span bounds, plus all ADR-0041 invariants.
2. Projection determinism: for generated documents and viewport sweeps, project(narrow) then project(wide) yields the canonical topology unchanged; merge provenance matches the declared ladder; the same fixtures enter the conformance corpus.
3. Migration goldens: representative v1 documents (classic preset, splits, collapsed states) migrate to v2 and round-trip; corrupted and unknown-variant behavior is unchanged.
4. Geometry fixtures cover flush lanes with safe areas, lane creation at both edges, full-height spans beside lanes, and the terminal degradation fallback.
5. Scrolling tests prove: `reveal` of off-screen Content settles the anchor so the target is visible; scroll commands are discrete and revisioned; flush lanes stay scroll-pinned; the anchor round-trips through persistence; and scrolling alone causes no topology change and no unrelated Content rerenders.
6. The guardrail gate stays zero-baseline; no new kernel imports; the React adapter remains the only CSS Grid consumer.
