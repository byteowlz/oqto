# The base24 design-system standard is vendored into oqto via `byt design-system sync`, not consumed as a published npm dependency

> **Status: Accepted. Supersedes [ADR-0014](0014-base24-design-system-as-published-npm-dep.md)** (delivery decision only; the color mechanism in 0014 stays).

ADR-0014 chose npm publish as the delivery channel for the `@byteowlz/design-system` standard, on the grounds that "zero-friction distribution" required a registry-resolved package. That reasoning was written imagining external, independent consumers. At oqto's actual scale — **one internal consumer, in the same workspace, sharing the same author** — npm publish is all cost and zero benefit. We reverse the delivery decision: the standard is **vendored into oqto by a managed sync command** (`byt design-system sync`), while the source repo (`byteowlz/design-system`) remains the single source of truth. The mechanism decisions from ADR-0014 (base24, the closed 16-role layer, derived alpha, effects as an Identity concern) are untouched and still accepted.

## Why this reverses cleanly

A "standard" only earns the cost of publishing (semver discipline, provenance, deprecation surface, org/login gating, CI) when it has **multiple, independent consumers that adopt it asynchronously**. oqto is the sole consumer, oqto drives the changes, and the source repo is local to the same workspace. At N=1:

- **There is no async adoption** — changes flow oqto → source repo, never the other way. Drift, the whole reason publishing exists to combat, cannot bite the driver.
- **The "zero-friction" goal is better met by a sync command** than by npm. No login, no `.npmrc`, no registry, no version bumps in two places — one command, `byt design-system sync`, and oqto is current.
- **Vendoring is reversible; publishing is sticky.** Going vendor→publish is a 1-line swap later. Going publish→vendor leaves a deprecated package lingering on the registry forever.
- **`byt` already owns this exact shape twice.** `byt schema` syncs files between a central repo and the catalog (aggregate-up); `byt templates` distributes down from `byteowlz/templates` into new repos (distribute-down). `byt design-system sync` is a distribute-down case — it generalizes an existing pattern, it is not a new architectural seam. See [`byt-smr6`](https://github.com/byteowlz/byt) for the command spec.

The npm login block that held up M1 for multiple sessions was the symptom of this mismatch, not the cause. The cause was choosing a multi-consumer delivery channel for a single-consumer problem.

## Decision

1. **Delivery channel: managed vendoring via `byt design-system sync`.** The source repo (`byteowlz/design-system`) stays the single source of truth — spec, ADRs, playground, reference impl all live there and retain their value. oqto pulls a pinned, reproducible snapshot of `impls/shadcn-ts/` into `frontend/vendor/design-system/` on demand.
2. **The snapshot is stamped, not silent.** Every sync writes `frontend/vendor/design-system/VENDORED.md` recording source URL, pinned ref, resolved sha, sync date, and byt version. A vendored copy is never a mystery; drift is queryable (`byt design-system status`), not guessed.
3. **npm publish remains available as a parallel channel**, not the channel. The source repo can serve internal consumers via `byt` *and* external ones via npm using the same files, the day an external consumer appears. Nothing is foreclosed; publishing is just no longer on the critical path.

### Two sub-decisions (baked into `byt-smr6`)

- **D1 — sync `src`, not `dist`.** `dist/` is gitignored in the source repo (a build artifact). Syncing source keeps the source repo's diffs clean, mirrors how a real dependency works, and avoids coupling `byt` to any toolchain. oqto builds the vendored TS in its own pipeline (`bun run build`).
- **D2 — whole package as a local path dep, not flattened files.** The vendored dir keeps its `package.json`; oqto references it as a workspace or `"file:./vendor/design-system"` entry. Imports stay clean (`@byteowlz/design-system`) and the day an external consumer appears, flipping to a real npm dep is a 1-line change in `package.json` — identical import surface.

## The trigger to flip back to npm publish

Publishing is deferred, not cancelled. The concrete trigger is: **a second consumer appears that is not byteowlz-internal** (e.g. an external tool, a public template, a docs site that needs the package), OR a byteowlz screen-UI tool that cannot run `byt` (different machine, no workspace). At that point `byt design-system sync` keeps working for internal consumers and npm publish is added as a parallel channel for the external one. This trigger is recorded so the flip is event-driven, not vibes-driven.

## Consequences

- `frontend/mini-apps/theming/` is still deleted and replaced by the standard — the engine *content* is unchanged from ADR-0014. What changes is *where oqto gets it*: a vendored snapshot under `frontend/vendor/design-system/` (a local path dep), not `bun add @byteowlz/design-system`.
- M1 (the cutover) is now gated on `byt-smr6` existing and the source repo being tagged `v0.1.0` (so byt has a ref to pin). It is **no longer gated on npm login or a first publish**.
- `frontend/vendor/design-system/` is committed to oqto (a real, buildable vendored dep), with `VENDORED.md` as the audit trail. It is regenerated by `byt design-system sync`, never hand-edited.
- No `@byteowlz` npm org, no first publish, no provenance setup is required to ship. Those become optional, deferred to the trigger above.
- The rest of ADR-0014 holds unchanged: oqto's shadcn components read the same variable names (no component rewrites), TUI/terminal stays out of scope, the two house schemes (`oqto-dark`, `oqto-light`) are fed to `applyScheme()`.
