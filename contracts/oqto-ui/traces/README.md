# OqtoUI conformance traces

Platform-neutral golden fixtures for OqtoUI behavior (ADR-0037, ADR-0041).
A second client implementation (e.g. a GPUI/Rust host) proves semantic
equivalence by consuming these JSON files alone — never the TypeScript
reference implementation that generated them.

## `compositor/` — Layout Engine transition corpus

The ADR-0041 compositor kernel (`frontend/src/oqto-ui/compositor/kernel/`)
is the reference implementation. Its canonical shapes — `LayoutSnapshot`,
`LayoutTransaction`/`LayoutCommand`, `LayoutEvent`, `LayoutRejection`,
persistence documents, and `SolvedLayout` — are versioned, pure JSON, and
contain no React, DOM, CSS, or renderer state. React/CSS Grid, browser
storage, pointer/keyboard input, and accessibility rendering are adapters
outside the corpus.

Three fixture formats, each self-describing via `format`, `formatVersion`,
and the layout `schemaVersion` it exercises:

- **`oqto-compositor-transitions`** (`transitions-*.json`): an `initial`
  snapshot plus ordered `steps`. Each step holds the semantic `transaction`
  and the `expected` typed result: `{ok: true, events, snapshot}` or
  `{ok: false, rejection}`. Replay each accepted step's `snapshot` as the
  next step's input. Curated traces cover open/tab/reveal/activate/focus/
  collapse/close, splits/moves/resize with grid repair, empty
  retain/collapse/remove behavior, revision conflicts, undo, and every
  rejection reason; `transitions-generated-seed-*.json` are seeded random
  sequences with full expected state at every step.
- **`oqto-compositor-geometry`** (`geometry-grid.json`): topology snapshots
  with viewport cases (including safe areas and infeasible minima) and the
  expected solved rects plus degradation reports. Geometry is derived
  renderer input, never persisted state.
- **`oqto-compositor-persistence`** (`persistence-documents.json`): raw
  document strings with the expected decode outcome — valid round trips
  (unknown future variants preserved) or a typed failure `reason`
  (`parse-error`, `invalid-shape`, `unsupported-version`,
  `invariant-violation`).

## Comparison rules

- **Structural, not byte:** parse JSON and compare values. Key order and
  whitespace are not part of the contract (the generator's byte-stable
  output is a convenience for diffs, not a requirement).
- **Numbers are IEEE-754 doubles.** Geometry values come from a fixed
  operation order in the solver; a conforming implementation using f64 with
  the same algorithm reproduces them exactly. If an implementation cannot,
  that is a solver-algorithm divergence, not a tolerance problem.
- **IDs are opaque strings.** `container-N`/`column-N` minting from the
  snapshot's `idSeed` is part of deterministic behavior and covered by the
  traces; content ids are caller-supplied stable identities.
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
independent replay of the committed JSON through the public seam must
reproduce every expected outcome. An intentional semantic change therefore
regenerates the corpus in the same commit, and the diff is the review
surface.

## Second-host policy (no premature shared core)

Per ADR-0037, Oqto does not extract a portable Rust core or impose WASM
speculatively. A GPUI/Rust compositor implementation must first pass this
corpus (transitions, geometry, persistence) plus its own native
interaction, performance, and accessibility gates; only after that
demonstrates enough shared behavior is extracting a shared core considered.
