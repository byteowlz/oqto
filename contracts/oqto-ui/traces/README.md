# OqtoUI conformance traces

Platform-neutral golden fixtures for OqtoUI behavior (ADR-0037, ADR-0041).
A second client implementation (e.g. a GPUI/Rust host) proves semantic
equivalence by consuming these JSON files alone — never the TypeScript
reference implementation that generated them.

## `compositor/` — Layout Engine transition corpus

The ADR-0041/0042/0043 compositor kernel
(`frontend/src/oqto-ui/compositor/kernel/`, layout schema v2) is the
reference implementation. Vocabulary: **Screen Mode → Arrangement → Lane
(row track) → Container → Content tabs**; a Screen Mode's layout document
is one `LayoutSnapshot` holding several Arrangements (each with row/column
tracks, spanned placements, a scroll anchor, and focus memory), an
`activeArrangementId`, and an opaque `activeWorkspace`. Its canonical
shapes — `LayoutSnapshot`, `LayoutTransaction`/`LayoutCommand` (optionally
carrying the `viewport` that makes reveal scrolling exact), `LayoutEvent`,
`LayoutRejection`, persistence documents, `SolvedLayout`, and
`ProjectedLayout` — are versioned, pure JSON, and contain no React, DOM,
CSS, or renderer state. React/CSS Grid, browser storage, pointer/keyboard
input, and accessibility rendering are adapters outside the corpus.

Four fixture formats, each self-describing via `format`, `formatVersion`
(currently 2), and the layout `schemaVersion` it exercises (2):

- **`oqto-compositor-transitions`** (`transitions-*.json`): an `initial`
  snapshot plus ordered `steps`. Each step holds the semantic `transaction`
  and the `expected` typed result: `{ok: true, events, snapshot}` or
  `{ok: false, rejection}`. Replay each accepted step's `snapshot` as the
  next step's input. Curated traces cover open/tab/reveal/activate/focus/
  collapse/close, splits/moves/resize with grid repair, empty
  retain/collapse/remove behavior, revision conflicts, undo, and every
  rejection reason, plus Lanes at both edges, flush lanes, scrolling
  (discrete `scroll` and viewport-aware `reveal`), Arrangement
  create/switch/remove with focus memory, reveal-teleport across
  Arrangements, work-directory binding, and active-Workspace switching;
  `transitions-generated-seed-*.json` are seeded random sequences with full
  expected state at every step. An `invariant-violation` rejection never
  appears as an expected outcome: it is the engine's defense-in-depth check
  and would mean a kernel bug.
- **`oqto-compositor-geometry`** (`geometry-grid.json`): topology snapshots
  with viewport cases (safe areas, spans, flush lanes, scroll offsets, and
  both column overflow policies: `scroll` honors minima and reports
  `overflow`; `fit` scales and reports `minima-unsatisfiable`) and the
  expected solved rects, track sizes, and scroll offset. Geometry is
  derived renderer input, never persisted state.
- **`oqto-compositor-projection`** (`projection-ladder.json`): canonical
  snapshots with `ViewportClass` cases (`responsive: "merge" | "scroll"`,
  widths from wide to tiny) and the expected non-destructive projection —
  projected snapshot, merge provenance, containers collapsed for space, and
  solved geometry. The input snapshot is never mutated; the ladder is
  collapse navigation → merge inline neighbors into their target's tabs per
  row by role priority → scale as the terminal fallback.
- **`oqto-compositor-persistence`** (`persistence-documents.json`): raw
  document strings with the expected decode outcome — valid round trips
  (unknown future variants preserved at every level), a schema v1 document
  migrated to v2, or a typed failure `reason` (`parse-error`,
  `invalid-shape`, `unsupported-version`, `invariant-violation`).

## Comparison rules

- **Structural, not byte:** parse JSON and compare values. Key order and
  whitespace are not part of the contract (the generator's byte-stable
  output is a convenience for diffs, not a requirement).
- **Numbers are IEEE-754 doubles.** Geometry values come from a fixed
  operation order in the solver; a conforming implementation using f64 with
  the same algorithm reproduces them exactly. If an implementation cannot,
  that is a solver-algorithm divergence, not a tolerance problem.
- **IDs are opaque strings.** `container-N`/`track-N`/`arrangement-N`
  minting from the snapshot's `idSeed` is part of deterministic behavior
  and covered by the traces; content ids are caller-supplied stable
  identities.
- **Absent vs null:** optional fields are omitted when absent (never
  serialized as `null` unless the schema says nullable, e.g.
  `focusedContentId`, `activeContentId`).

## Regeneration and drift

Regenerate from the reference implementation with:

```sh
cd frontend && bun scripts/generate-compositor-traces.ts
```

Regeneration is deterministic (seeded, no wall clock). The vitest suite
`frontend/tests/compositor/oqto-ui-compositor-traces.test.ts` enforces both
directions: the committed corpus must equal a fresh regeneration, and an
independent replay of the committed JSON through the public seam
(`applyTransaction`, `solveLayoutGeometry`, `projectLayout`,
`recoverLayoutDocument`) must reproduce every expected outcome. An intentional semantic change therefore
regenerates the corpus in the same commit, and the diff is the review
surface.

## Second-host policy (no premature shared core)

Per ADR-0037, Oqto does not extract a portable Rust core or impose WASM
speculatively. A GPUI/Rust compositor implementation must first pass this
corpus (transitions, geometry, persistence) plus its own native
interaction, performance, and accessibility gates; only after that
demonstrates enough shared behavior is extracting a shared core considered.
