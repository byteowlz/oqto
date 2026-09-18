# Layout presets: recipes over the semantic grid, side-panel modes, and the layout switch

## Status

Proposed (2026-09-18). Builds on [ADR-0042](0042-semantic-grid-v2-lanes-spans-responsive-projection.md) (tracks, placements, spans, flush Lanes) and [ADR-0043](0043-arrangements-scrolling-binding-overview.md) (Arrangements); chrome placement follows [ADR-0040](0040-oqto-customizations-lua-configurable-ui.md) (Slots). Applies to every host of the compositor kernel: the web shell and the native desktop client (`oqto-desktop`, whose kernel is a port proven by the shared trace corpus).

Source material: `design-exploration/Mockups/20260918_oqto_UI_mockups_layout_desktop_*.png`.

## Vocabulary

| Term | Meaning |
| --- | --- |
| **Layout preset** | A named **recipe** that compiles to an ADR-0042 grid topology and re-slots the active Arrangement's Content into it. Data, never code in a host. |
| **Recipe** | The small set of choices a preset is made of: side-panel modes, primary column count, primary row count, top and bottom Lane modes. |
| **Side panel** | The left (navigation-role) or right (auxiliary-role) column of the frame. |
| **Sidebar mode** | A side panel as one Container in a flush column spanning every row, including the status row. Not splittable. |
| **Card mode** | A side panel as an inset column inside the gap rhythm, like any other pane. Splittable into rows. |
| **Primary area** | The columns between the side panels. Each column is an independent Container by default; three columns are three sessions side by side. |
| **Strip** | A top or bottom Lane (ADR-0042) that is either **spanning** (one Container across all primary columns) or **per column** (one Container under or over each primary column). |
| **Layout switch** | The chrome control that opens the layout modal. A Slot, never Content. |
| **Layout modal** | The picker: a Pinned tab (the user's rotation) and an All tab (every available preset, with pin toggles). |
| **Pinned** | A preset in the user's rotation. Cycling and jump keys operate on pinned presets only. |

## Context

ADR-0042 made the grid expressive enough for lanes, spans and full-height sidebars, and ADR-0043 made whole layouts switchable as Arrangements. What is still missing is a shared, finite description of *which* layouts a user is offered and how switching between them treats existing Content. Without it the web shell and the desktop client will each invent their own preset lists and their own re-slotting rules, and the trace corpus cannot prove they agree.

The 2026-09-18 mockups settle the family. Every desktop layout keeps one frame and varies only the primary area:

| Mockup | Primary columns | Top strip | Bottom strip |
| --- | --- | --- | --- |
| classic | 1 | – | – |
| classic, sidebar as card | 1 | – | – |
| center stage | 1 | spanning | spanning |
| center stage double | 2 | spanning | spanning |
| column rows 1 | 3 | – | per column |
| column rows 2 | 3 | per column | per column |
| command center | 3 | – | spanning |
| gallery | 3 columns × 2 rows | – | – |

## Decision

### A preset is a recipe; the kernel compiles it

A recipe has exactly these fields:

- `left`, `right`: `sidebar` \| `card { rows: 1..=3 }` \| `hidden`
- `primary_columns`: 1..=3
- `primary_rows`: 1..=2 (2 only for gallery-style equal rows)
- `top`, `bottom`: `none` \| `spanning` \| `per_column`

The kernel exposes `recipe_topology(recipe) -> GridTopology` and `apply_recipe(snapshot, recipe) -> snapshot`. Both are pure; both are exercised by the conformance corpus so every host produces the same geometry and the same Content assignment. Hosts never hand-build topologies for presets.

Compilation rules:

- **Sidebar mode** is one Container in a column whose first placement spans every row; the status Lane's placement starts after that column. **Card mode** is an inset column with `rows` placements; a card-mode panel is the only side panel that `split` at a block edge may divide further. Switching a split panel to sidebar mode merges its Containers into one tab list in declared order.
- The right panel may be in sidebar mode too; nothing in the model privileges the left edge beyond the default recipe.
- **Strips** are Lanes with a fixed block size and a minimum. Spanning strips get `colSpan = primary_columns`; per-column strips are one Container per primary column in the same row track. Strips are not flush by default; a preset may declare them flush under ADR-0042's rules.
- **Gallery** is `primary_rows = 2` with equal fraction rows and no strips.
- A fourth primary column is rejected as configuration. The mockups stop at three, and the ADR-0042 merge ladder has no answer for more.

### Switching a preset never loses Content

`apply_recipe` re-slots the active Arrangement in place; it does not create an Arrangement (ADR-0043 owns that) and it does not open or close Content:

1. Containers are matched to slots by role first: navigation → left, auxiliary → right, status → status Lane.
2. Primary Containers fill primary columns left to right, then top to bottom in gallery; strips take Containers that were previously in top or bottom Lanes, in order.
3. Overflow Containers merge into the last slot of their kind as tabs, preserving order and the active tab.
4. Slots the old Arrangement cannot fill are created empty with the classic preset's `EmptyBehavior`.

Reversal is exact for the same Content set: classic → columns → classic restores the original placements. The corpus carries a preset-cycle trace proving this.

### Named presets are data

The shipped names and recipes are `classic`, `center-stage`, `double`, `columns`, `command-center`, `gallery`, in that order, matching the mockups; `classic` is the default. The list lives with the Screen Mode's presentation data so ADR-0040 Customizations can add or reorder presets without host changes. The active preset name persists with the Arrangement; when a user edits the grid by hand the Arrangement is simply "custom" until a preset is applied again.

### The layout switch is a Slot that opens a modal

The layout switch is chrome: a button in the status bar Slot (desktop and web) and in the mobile Screen Mode's toolbar. Activating it, or the layout shortcut, opens the **layout modal**. The modal has two tabs:

- **Pinned:** the user's rotation, in the user's order, each preset drawn as a miniature of its solved geometry with its jump key. Activating one applies it and closes the modal.
- **All:** every available preset — the shipped six plus any ADR-0040 Customization presets — with a pin toggle on each. Pinning appends to the rotation; unpinning removes it. The shipped six are pinned by default.

Cycling commands (keyboard, and Agent requests) iterate **the pinned set only**, in pinned order; jump keys select pinned slots one through nine. Within the modal, arrow and vim keys move, a single key toggles the pin, Enter applies, Escape closes.

Pins and their order persist as presentation data for the user and Screen Mode, never in the layout document, so they survive Arrangement switches and Workspace changes. Whether the modal is open is host state: not persisted, not part of the layout document, and not something an Agent can toggle. Agents may request a preset through the typed control surface of ADR-0041 with the same disruption limits as `reveal`.

### Content kinds are open

Columns are Containers and hold any Content: sessions, files, git, trx, terminals, and, once hosts support them, ADR-0038 apps. The preset family does not depend on the kind of Content; three columns are three of anything.

## Consequences

- One preset list and one re-slotting rule on every host, proven by the shared corpus rather than by screenshots.
- The desktop client needs no layout logic of its own beyond picking a recipe and drawing the switch.
- Side-panel splitting is a first-class, mode-gated capability rather than an accident of the generic `split` command.
- Cost: the kernel gains a recipe compiler and the corpus gains a preset-cycle trace and per-preset placement goldens.

## Alternatives considered

- **Presets as stored Arrangement documents:** switching would then be a document swap that loses Content or duplicates it; a recipe over the live Arrangement keeps sessions where they are.
- **Free-form only, no presets:** users cannot rebuild a three-column workbench quickly, and hosts diverge on defaults.
- **Host-specific preset lists:** rejected for the reason this ADR exists.

## Validation

1. Kernel: for each of the eight mockups a recipe whose compiled topology matches a golden placement table.
2. Kernel: property tests over random Content sets that `apply_recipe` preserves every Content reference and active tab, and that applying the previous recipe restores the previous placements.
3. Corpus: a preset-cycle trace consumed by both the TypeScript kernel and the Rust port.
4. Host: with three sessions open, cycling every pinned preset never ends a session or loses composer text; split controls are disabled on a sidebar-mode panel.
5. Host: pins survive a relaunch and an Arrangement switch; unpinning the active preset keeps it active and drops it from the rotation.
