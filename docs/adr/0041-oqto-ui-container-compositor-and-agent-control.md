# OqtoUI is a Container compositor with optional agent control

## Status

Accepted (2026-09-01). Tracked by `oqto-a9j4.8`. Refined by [ADR-0042](0042-semantic-grid-v2-lanes-spans-responsive-projection.md) (grid v2: lanes, spans, scrolling, responsive projection) and [ADR-0043](0043-arrangements-scrolling-binding-overview.md) (Arrangements, Workspace switching, overview; renames Layout Profile to Screen Mode). Refines and partially supersedes [ADR-0037](0037-oqto-ui-portable-rearrangeable-views.md); companion decisions are [ADR-0038](0038-agent-built-apps-installation-binding-and-filesystem-discovery.md) for Apps and [ADR-0040](0040-oqto-customizations-lua-configurable-ui.md) for declarative shell arrangement.

This ADR supersedes ADR-0037's desktop layout as a persisted tree of split nodes and tab stacks. It preserves OqtoUI's platform-neutral identity, stable presentation identity, progressive fidelity, requester-local Screen Modes (this ADR's original "layout profiles"), mobile projection, native rendering, placement-independent ownership and authority, and parallel replacement of the old interface.

## Context

A left full-height sidebar, a central Chat region, and a right auxiliary region are not three hardcoded feature panels. They are Containers with different placement constraints. Chat, Files, Git operations, Terminal, and an App Instance presentation are peer kinds of Container Content; one may occupy a Container alone or appear as a tab in its content stack. The grid must later admit additional Containers and content without making each arrangement a new shell implementation.

Modern compositors such as Niri, Hyprland, and Sway/i3 demonstrate useful architectural separation: stable content identity is independent from placement; focus is independent from visibility; semantic commands mutate a deterministic layout model; placement rules are separate from layout algorithms; and rendering is a projection rather than the source of truth. OqtoUI adopts these mechanisms, not their desktop metaphor or visual presentation.

Directly encoding the product in React panels, CSS Grid coordinates, or feature-owned stores would make rearrangement, recovery, accessibility, native hosts, and agent control unreliable. Conversely, exposing a compositor's raw state, DOM, or control socket to an Agent would bypass requester-local routing and make stale or over-broad mutations possible.

## Decision

### OqtoUI composes Containers containing Container Content

The canonical composition hierarchy is:

```text
Screen Mode
  -> layout strategy (Grid first)
    -> Container
      -> ordered Container Content stack
        -> active Content presentation
```

A **Container** is a stable, grid-managed shell region. It owns placement, span, sizing constraints, full-height behavior, collapse/resize behavior, chrome, content ordering, the active content reference, and the fidelity allocated to that content. It owns no Chat messages, files, Git state, App state, binding, grant, or server authority.

**Container Content** is one addressable presentation item eligible for placement in a Container. First-party kinds include Sessions, Chat, Files, Git operations, Terminal, Editor, and resource previews. App Content references an App Instance and one of its declared presentations under ADR-0038. A tab is only the shell control selecting one Content item in a Container stack; it is not content or an authority boundary. With one item the tab strip may be hidden.

An App Definition, Installation, and Instance are not layout objects. Only a requester-local presentation of an App Instance becomes Container Content. Closing or moving it changes presentation only; it never deletes the Instance, changes its binding, or changes a grant. First-party and App Content share placement mechanics while retaining their distinct implementation and trust models.

The initial classic desktop Preset has a full-height navigation Container, a primary Container containing Chat, and an auxiliary Container containing Files or other Content. These are preset data, not permanent feature regions. Dropping Content onto another Container tabs it; dropping it at a supported edge atomically creates a Container/grid track and moves the Content into it. Git operations are a first-party Content kind, not a Container kind.

Each Container declares an empty behavior: **retain** renders its explicit empty state, **collapse** preserves it without consuming its normal track, and **remove** deletes that Container and repairs the grid when its last Content leaves. Preset navigation, primary, and auxiliary Containers retain or collapse; dynamically created split Containers remove by default. An empty Container has no active Content, and an empty Layout has no focused target.

Layout state contains only placed Content references. Closing Content removes that reference; any pinned restoration descriptor and presentation state live with their existing presentation-state owner outside the layout topology. An authorized owner/resource resolver may project available but unplaced presentation descriptors; this is not a second store, authority, or ADR-0040 Provider catalog. Agents cannot use a stale placement reference to reach content they were not authorized to inspect.

### A deterministic Layout Engine is the sole mutation seam

The compositor kernel is a framework-independent deep module. It accepts a versioned Layout snapshot plus an atomic semantic transaction and returns either a new snapshot with semantic events or a typed rejection. It imports no React, DOM, CSS, network, feature store, or persistence implementation. Renderers and gestures are adapters at its interface.

All human input, Customizations, restored state, and optional Agent control use the same command vocabulary: open, reveal, activate, focus, move, split, resize, collapse, close, restore, preview, apply, and undo. React code never mutates Container arrays or persisted layout documents directly. Multi-command transactions apply completely or not at all and use an expected Layout revision; stale mutations return conflicts rather than overwriting newer human interaction.

`open` is idempotent for an exact stable Content identity: if it is already placed, the default behavior reveals it. Creating a second presentation of the same Session, resource, or App Instance requires an explicitly minted distinct Content identity and remains subject to multiplicity metadata in the host-owned Content kind registry, not a Layout Preset; an App Definition may declare support, but host policy remains authoritative. `reveal` accepts only already placed Content and returns a typed not-placed result otherwise; it never creates a duplicate.

The model keeps topology, Container membership, active content, focus, viewport visibility, and attention separate. Revealing Content makes it visible but does not imply keyboard focus. Animations and solved/observed geometry are renderer output and never authoritative state. Layout documents may contain declared semantic sizes, track ratios, and host-profile-local logical lengths, but never observed rectangles or DOM measurements. They otherwise contain stable references, constraints, ordering, schema/revision data, and migration provenance—not messages, file contents, App data, secrets, sockets, or grants.

Every accepted transition preserves at least these invariants:

1. every Content reference present in Layout state belongs to exactly one Container;
2. a non-empty Container's active Content belongs to its stack, while an empty Container has no active Content;
3. focus is absent for an empty Layout and otherwise references an existing reachable target;
4. Container and Content identities survive moves and resizing;
5. transactions are atomic and revision-checked;
6. presentation changes never change content ownership, App binding, capability, or authorization;
7. unknown future Container and Content variants survive persistence round trips;
8. solved geometry is derived from logical topology and host constraints;
9. closing presentation does not delete its domain owner; and
10. a stale Agent transaction cannot overwrite newer user work.

### Grid is the first strategy, not the wire format of CSS Grid

The first layout strategy is a semantic grid with versioned logical tracks, placements, spans, and constraints such as full height, minimum/preferred size, resizable, and collapsible. The web adapter projects solved geometry to CSS Grid; future native hosts use native layout. Persisted state never contains CSS selectors, observed DOM geometry, `vh` assumptions, or renderer-specific expressions. Full height means spanning the root block axis while respecting the host's safe areas. When a viewport cannot satisfy all declared minima, the profile's deterministic overflow/collapse priority applies and the solver reports degradation; it never emits overlapping, negative, or inaccessible geometry silently.

Grid placement, geometry solving, directional navigation, and initial placement policy remain internal parts of the Layout Engine. A second layout strategy—such as scrolling columns, master/detail, or focused presentation—earns a public strategy seam only when a real second implementation is required. Oqto does not prebuild speculative compositor modes.

Desktop, tablet, mobile, and Big Picture remain separate Screen Modes over the same Content identities. Mobile projects available Content into one focused destination, drawers, sheets, and switchers rather than shrinking the desktop grid. A host reports what semantic request it actually satisfied.

### Agent compositor control is optional, typed, and requester-local

Oqto may expose compositor control to Agents through canonical runner capability-broker operations, with native Pi tools and optional MCP/headless adapters over the same contracts. This is analogous to a typed compositor dispatcher, not a raw frontend control socket. Agents never receive DOM access, CSS selectors, React store access, arbitrary WebSocket access, or authority to name an Account, client connection, principal, grant, or Presentation Context.

The capability distinguishes inspection, opening, revealing, focusing, arranging, closing, previewing, applying, and undoing. These are separately policy-controlled because making Content visible, stealing keyboard focus, rearranging a workspace, and closing a presentation have different disruption and disclosure risks. Semantic placement intents such as tab in a known Container, split at an edge, or prefer a primary/auxiliary role are allowed; raw host coordinates are not the normal Agent interface. Focus/reveal requests are rate- and oscillation-limited so a granted Agent cannot make the interface unusable.

A **Presentation Context** is derived from the authenticated interaction's originating connected requester-local OqtoUI client, not merely its Account or Session. The broker attaches it; the Agent cannot provide or redirect it. Container and Content placement IDs returned by inspection are opaque, scoped to that Presentation Context and Layout revision, and rejected when replayed against another client, Screen Mode, or stale context. If the originating client is gone or no eligible live presentation exists, the operation fails explicitly or follows a separately approved notification workflow; another tab/device is never guessed. Multiple clients do not share mutable layout implicitly.

Agent transactions carry the observed Layout revision, are validated by the same engine and policy as human actions, and report the actual applied placement. Human interaction wins stale-revision conflicts. Nontrivial transactions can be previewed before application. An applied Agent transaction returns an attributable undo handle only for that transaction; undo requires the expected resulting revision, cannot target a human transaction, and is rejected after intervening incompatible work. An App does not inherit Agent compositor control: Apps may receive only narrow Host presentation/navigation capabilities, and cannot inspect or rearrange unrelated Containers.

If an App Instance, resource owner, membership, or grant becomes unavailable while its Content remains placed, the Host stops/suspends the presentation as required, preserves placement as an explicit unavailable item, and revalidates authority before restoration. Placement never keeps an iframe or capability alive after authority is lost.

### Guardrails precede compositor implementation

No compositor UI or Agent compositor capability is implemented before executable architecture guardrails exist. Compositor-specific checks extend or are delegated from ADR-0037's zero-baseline OqtoUI guardrail gate; they never broaden or weaken the still-scoped Workbench checker, which remains until that prototype is deleted. The checks enforce dependency direction, ban React/DOM/network/persistence imports from the kernel, ban feature-owned direct layout mutation, reject `any` and handwritten mirrors in canonical contracts, require versioned persistence and migrations, and keep App/Agent adapters behind the public Layout Engine interface.

The kernel begins with invariant assertions, model/property generators, transaction atomicity and revision-conflict tests, serialization/migration and last-known-good recovery tests, unknown-variant tests, authorization-negative tests, focus/accessibility tests, and representative performance fixtures. Generated sequences cover open, tab, split, move, focus, resize, collapse, reveal, close, undo, serialize, and restore. Renderer tests prove local layout changes do not rerender or refetch unrelated Content.

Oqto App runtime support does not wait for the new compositor. The old interface may open an App Instance presentation through a narrow legacy presentation adapter so Apps can be tested now. That adapter consumes the same stable App Instance/presentation identity and must not establish legacy tabs, paths, iframe state, or React stores as canonical layout authority. Compositor code may not depend on this adapter, and its removal remains an explicit OqtoUI cutover criterion.

## Consequences

- OqtoUI becomes a content compositor rather than a dashboard layout library; CSS Grid and React are adapters, not the product model.
- Container placement and Content ownership become independent. App Content can open alone, as a tab, or move between Containers without changing App lifecycle or authority.
- The first-party shell, Customizations, human gestures, and Agents converge on one semantic transaction interface.
- Robustness work moves ahead of visible drag/drop implementation, adding initial cost but preventing multiple incompatible layout stores and unsafe Agent shortcuts.
- Agent presentation control is useful but optional, capability-discovered, requester-local, revisioned, policy-limited, attributable, and unavailable rather than guessed in headless contexts.
- The immediate App-runtime vertical slice targets the old interface through an explicit migration adapter; it does not block on or prematurely implement the compositor.

## Rejected alternatives

- **Hardcoded left/Chat/right panels:** preserves the current screenshot but prevents a general grid and additional Content kinds.
- **Treat each App or feature as its own Container type:** conflates shell geometry with content and duplicates tab, focus, resize, persistence, and mobile behavior.
- **Persist CSS Grid or DOM geometry:** ties canonical state to one renderer and makes recovery and native hosts brittle.
- **Make a split tree permanently canonical:** useful for tiling window managers, but it makes full-height spanning and the intended grid Presets indirect; split remains a semantic operation over the grid strategy.
- **Let features mutate a shared layout store:** creates ordering races and makes atomic transactions, replay, and property testing impossible.
- **Expose raw compositor IPC to Agents:** bypasses requester-local routing, revision checks, authorization, semantic portability, and host policy.
- **Give Apps compositor authority:** allows sandboxed content to inspect or disrupt unrelated shell presentation; Apps provide Content, while the Host controls placement.
- **Block App runtime work on the compositor:** delays real App validation and encourages designing the compositor without working App Content.

## Verification

Before compositor implementation advances beyond contracts and guardrails, evidence must show:

1. architecture checks fail on forbidden kernel imports, direct state mutation, unversioned persistence, `any`, and App/Agent bypasses;
2. model/property tests preserve every invariant across generated command sequences—including empty retain/collapse/remove behavior, idempotent open, explicit duplicate presentations, and reveal-not-placed—and prove failed transactions leave state unchanged;
3. stale human/Agent concurrency, persistence corruption, migrations, unknown variants, and last-known-good recovery have deterministic tests;
4. semantic grid fixtures solve full-height navigation, primary Chat, auxiliary tabs, splits, moves, collapse, constraint infeasibility, and safe-area projection without persisted renderer geometry;
5. first-party and App Content follow identical placement transitions while App binding/grant revocation, unavailable-owner, and suspended-Instance tests prove placement retains no authority or live sandbox;
6. Agent inspection, reveal, focus, arrange, close, preview, apply, and transaction-scoped undo honor separate policy, originating-client routing, context-scoped ID rejection, expected revisions, disruption limits, explicit headless failure, and typed actual outcomes;
7. keyboard/directional focus, focus restoration, screen-reader order, mobile projection, reduced motion, and minimum touch targets pass per-host accessibility tests;
8. representative many-Container/many-Content fixtures meet budgets fixed before implementation and local mutations cause no unrelated Content rerenders or data refetches; and
9. the old-interface App adapter opens a real App Instance presentation without making legacy UI state authoritative; an architecture check forbids compositor dependencies on it, and OqtoUI cutover tracks its deletion explicitly.
