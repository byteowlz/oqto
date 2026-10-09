# Arrangements: switchable layouts, work-directory binding, Workspace switching, overview, and the switcher rail

## Status

Accepted (2026-09-01). Tracked by `oqto-a9j4.12`. Builds on [ADR-0042](0042-semantic-grid-v2-lanes-spans-responsive-projection.md) (grid v2 and horizontal scrolling within one Arrangement) and [ADR-0041](0041-oqto-ui-container-compositor-and-agent-control.md); companions are [ADR-0037](0037-oqto-ui-portable-rearrangeable-views.md) (`.oqto/` presentation data, Screen Modes) and [ADR-0040](0040-oqto-customizations-lua-configurable-ui.md) (Slots, Providers, Actions). Ships in the same layout `schemaVersion` 2 migration as ADR-0042.

## Vocabulary

The layout vocabulary reads top-down in one breath: **Screen Mode → Arrangement → Lane → Container → Content**.

| Term | Meaning |
| --- | --- |
| **Screen Mode** | The device/input class a layout is kept for: desktop, tablet, mobile, Big Picture. Renames ADR-0037's "Layout Profile"; semantics unchanged — each Screen Mode keeps its own persisted layout document and they never overwrite one another. |
| **Arrangement** | One complete layout the user can switch between, optionally bound to a work directory. A Screen Mode's document holds several, one active. |
| **Lane** | A horizontal strip of Containers inside an Arrangement (ADR-0042); top/bottom lanes may be flush. |
| **Container**, **Content**, **tabs** | Unchanged from ADR-0041. "Tabs" is a Container's ordered Content list; "stack" is used for nothing else. |
| **scroll anchor** | Where an Arrangement wider than the screen is currently scrolled to (ADR-0042). Not a separate noun in prose — an Arrangement simply scrolls. |
| **active Workspace / All** | Which Workspace the shell is currently showing. Oqto's existing Workspace entity; no new term. |
| **Slot** | ADR-0040 chrome mount point, e.g. the switcher rail. |

## Context

One grid layout is not enough working surface. Users organize parallel work — in Oqto most naturally per work directory — and want to switch whole layouts the way compositor users switch workspaces, with an overview to see everything at once. ADR-0041's composition hierarchy (`Layout Profile -> layout strategy -> Container`) has no level for "several layouts, one active." Oqto's terminology makes naming delicate: **Workspace**, **View**, and **Surface** are taken, and a Container's ordered Content list is already called its stack; hence *Arrangement*.

Workspaces are already Oqto's tenancy boundary. Users want to deliberately work inside one Workspace at a time, and sometimes to see everything across Workspaces — without the shell becoming a second authorization system.

Constraints carried forward: placement never confers authority (ADR-0041); Screen Modes are separate and never overwrite one another (ADR-0037); chrome is Slot territory, not Content (ADR-0040); Agents get only typed, revisioned, requester-local control (ADR-0041).

## Decision

### An Arrangement is one complete layout; a Screen Mode holds several

The composition hierarchy gains one level:

```text
Screen Mode
  -> Arrangement (one active)
    -> grid strategy (ADR-0042 lanes/tracks/placements + scroll anchor)
      -> Container
        -> ordered Content tabs
```

An **Arrangement** is a named, complete grid layout — its lanes, tracks, placements, Containers, its own scroll anchor, and its own last-focused-Content memory. The Screen Mode's layout document holds an ordered list of Arrangements plus `activeArrangementId`.

Switching Arrangements restores that Arrangement's scroll anchor and focus memory; the document's single global focus always references Content in the active Arrangement. Both persist with the Screen Mode's document, so reopening lands exactly where the user left each layout. Creating, switching, and removing Arrangements are ordinary semantic commands — revisioned, atomic, undoable, trace-covered. Removing a non-empty Arrangement requires explicit confirmation policy at the host; the kernel refuses to remove the last remaining Arrangement.

### Binding is an organizational reference, never authority

An Arrangement may carry a **binding**: an opaque, optional reference such as `{ kind: "work-directory", id }` or `{ kind: "workspace", id }`. Work directories are the first binding kind, fitting Oqto's work-directory-centric grain; a Workspace-bound Arrangement (a layout for the Workspace as a whole) and other kinds may follow. A binding buys host conveniences only: opening Content owned by a bound work directory defaults into its Arrangement, and switching work-directory context can switch Arrangements. It grants nothing, gates nothing, and its removal changes presentation only. Several Arrangements may share a binding, and unbound Arrangements are fully supported — the flexibility is the point.

### The active Workspace is a presentation filter; authority stays where it is

Ownership, membership, and placement isolation live in the backend and the Gate, not in layout. The shell adds an **active Workspace** on top as a pure presentation filter — one Workspace, or **All** — persisted in the layout document beside `activeArrangementId` and changed only by an explicit semantic command. It never enumerates anything the Account is not authorized to see, and **All** unions only the requester's own authorized Arrangements; the compositor is not an enforcer and showing is not granting.

With one active Workspace, the switcher rail, overview, and Arrangement-cycling commands present only that Workspace's Arrangements; under **All**, they present every Arrangement grouped by Workspace, and the overview gains a second zoom level — scroll through Workspaces, each showing its Arrangements. Grouping is derived by resolving each Arrangement's binding through the authorized owner/resource resolver (a work directory resolves to its Workspace); it is never stored redundantly in the layout document, and Arrangements whose owner cannot currently be resolved group under an explicit unavailable section rather than disappearing.

Switching the active Workspace is deliberate. A human-initiated `reveal` of Content placed in another Workspace may cross over — the interaction itself is the deliberate act — and the resulting events report the switch explicitly. An Agent `reveal` or transaction that would change the active Workspace requires its own policy gate on top of the Arrangement-switch gate, because moving a user between Workspaces is the highest disruption class the compositor knows. All layout state remains one requester-local document per Screen Mode: the active Workspace filters presentation; it never splits the document.

### One placement globally; reveal teleports

The single-placement invariant extends across the whole document: a Content identity is placed in exactly one Container in exactly one Arrangement. `reveal` therefore composes its effects atomically — switch to the placing Arrangement (and Workspace, for a human), scroll so the Container is visible, activate the Content in its Container — and remains idempotent and focus-neutral. A deliberate second presentation of the same owner still requires an explicitly minted distinct identity (ADR-0041); nothing is duplicated implicitly. `move` gains an optional destination Arrangement, used by overview drag and semantic commands alike.

### Overview is a projection, not layout state

The zoom-out overview renders every Arrangement's solved geometry, scaled, in a scrollable strip; activating one switches to it, and dragging Content between miniatures issues the extended `move`. Overview is a host presentation mode over pure solver output — whether it is open is host state, never part of the layout document, never persisted, and never something an Agent can toggle. The mobile Screen Mode's Arrangement switcher is the same projection in different chrome.

### The switcher rail is a Slot, with the NavigationRail as interim

The collapsed left edge carries the Arrangement switcher: one icon per Arrangement — for work-directory-bound Arrangements, the icon and display name come from the `.oqto/` presentation data that ADR-0037 already validates (relative, non-executable, deterministic fallbacks such as initials). Tapping an icon issues the Arrangement-switch Action; a hold may open that work directory's session picker per the corner-mode gesture vocabulary. Under **All**, the rail groups icons by Workspace.

Its target implementation is an ADR-0040 **Slot** (the inventory's "rail"/edge strip): a named chrome mount point fed by the baseline work-directories Provider, items bound to Actions, user-configurable as data. It is chrome by design — it persists across Arrangement switches, occupies no grid track, does not scroll with the Arrangement (the same pinning rule as flush lanes), and cannot be closed, tabbed, or dragged like Content. Until the Customizations plane ships, the existing OqtoUI `NavigationRail` component serves as the interim implementation of the same behavior; this interim is explicitly not a second canonical layout store and is replaced, not migrated, when Slots land.

The expanded navigation Container is unchanged by this ADR: full Sessions/work-directory navigation remains ordinary Container Content with ADR-0037 progressive fidelity.

### Agents and Arrangements

Optional Agent compositor control (ADR-0041) extends naturally: inspection reports Arrangement-scoped opaque IDs; reveal may cross Arrangements under the same disruption limits; Arrangement switching and removal are separately policy-gated because yanking a user to another layout is more disruptive than revealing a tab; changing the active Workspace is gated again on top. Nothing here weakens the requester-local Presentation Context, revision checks, or the rule that stale Agent transactions lose to newer human work.

## Consequences

- The layout document becomes a small forest instead of one tree; invariants, persistence, migrations, and the conformance corpus extend once, in the same schemaVersion 2 change as ADR-0042.
- Reveal-teleport makes every Session, file view, or App presentation reachable by identity from anywhere — the property Agent control and pickers want anyway.
- Work-directory-bound Arrangements give Oqto a workspace-switching model that matches its own domain grain without inventing new authority.
- "Layout Profile" is renamed **Screen Mode** across OqtoUI documents (ADR-0037, ADR-0041, `CONTEXT.md`'s Presentation Context) on acceptance; Big Picture is simply a Screen Mode.
- The rail-as-Slot decision front-loads a real second consumer for the ADR-0040 Slot/Provider/Action primitives.

## Rejected alternatives

- **Separate persisted documents per Arrangement or per Workspace:** breaks atomic cross-layout moves, reveal-teleport, and All; multiplies recovery paths; invites divergent revisions.
- **"Stack", "Lane Stack", "Workspace", "View", or "Profile" as names:** collide with established Oqto terms or with a Container's Content stack; "Display Mode" collides with the web `display-mode` media query.
- **Duplicating Content into every Arrangement:** violates single placement, reintroduces reconciliation-by-copy, and breaks reveal idempotence.
- **Pinned global Containers instead of a Slot rail:** a dock that is Content can be closed, moved, or lost; a switcher must be chrome.
- **Binding as capability:** placement must never grant authority; an Arrangement bound to a work directory the Account loses access to simply renders its Content unavailable per ADR-0041.
- **The active Workspace as an authority boundary:** duplicating tenancy enforcement in the shell invites divergence from the Gate and false security; hiding is not denying.
- **Storing Workspace ids in the layout document:** grouping is derivable from bindings via the resolver; storing it would go stale and create a second source of truth.
- **Overview as layout state:** persisted zoom would be renderer state leaking into the canonical model.

## Verification

1. Property tests over multi-Arrangement documents: global single placement, per-Arrangement invariants, focus-follows-active-Arrangement, last-Arrangement removal refused, cross-Arrangement move atomicity, and unchanged state on rejection.
2. Reveal-teleport fixtures in the conformance corpus: reveal from another Arrangement switches, scrolls, activates, and is idempotent; a second reveal is a no-op arriving at the same state.
3. Persistence: per-Arrangement scroll anchors and focus memory round-trip; v1 documents migrate to a single-Arrangement v2 document with identity preserved; unknown Arrangement-level variants survive.
4. Overview projection is pure: identical input documents yield identical miniature geometry; opening overview causes no layout transaction.
5. Rail behavior: icons resolve from `.oqto/` data with deterministic fallbacks; switch Actions route through the ordinary transaction seam; the rail never appears in layout documents.
6. Active-Workspace tests: rail/overview/cycling enumerate only the active Workspace's Arrangements; All groups by resolved Workspace with unresolved bindings in an explicit unavailable group; human cross-Workspace reveal reports the switch in its events; the active Workspace round-trips through persistence; filtering provably never widens the set of resolvable owners.
7. Agent-negative tests: Arrangement switch/removal denied without the specific policy grant; active-Workspace changes denied without the additional gate; stale cross-Arrangement transactions conflict.
