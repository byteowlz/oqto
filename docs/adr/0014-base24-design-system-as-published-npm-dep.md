> **Status: SUPERSEDED by [ADR-0017](0017-design-system-vendored-via-byt-not-npm-publish.md).** The npm-publish delivery decision is reversed: at one internal consumer, npm is all tax and zero benefit. The standard is instead vendored into oqto via `byt design-system sync` (managed vendoring, repo stays single source of truth). The color-mechanism decisions in this ADR (base24, closed 16-role layer, derive alpha, effects are Identity) **remain accepted** — only the *delivery channel* is overturned.

# Oqto's appearance is driven by the cross-tool base24 design-system standard, consumed as a published npm dependency

Oqto's frontend colors are currently hand-tuned shadcn CSS variables in `globals.css` (`:root` / `.dark`). We replace that with the shared `@byteowlz/design-system` standard: a base24 color engine + a closed abstract role layer + derived alpha, distributed as a normal npm dependency. Oqto's current dark/light look becomes two house schemes (`oqto-dark`, `oqto-light`) fed to the engine; nothing visually changes at the default until a user picks another scheme. The standard lives in its own repo (`byteowlz/design-system`) — oqto is a *consumer*, never the home. The mini-apps SDK (`frontend/mini-apps/`) is a separate consumer and is out of scope for this decision.

## Why a published npm dep, not a git dep or a vendored copy

The standard must be reusable across all screen-UI tools, not just oqto, so it needs a single source of truth adopted independently per repo. Two options were rejected:

- **`git+https` / `git+ssh` git dependencies.** Killed by ecosystem reality: **npm v12 (July 2026) defaults `--allow-git` to `none`** — git deps stop resolving unless every consumer opts in per-install (GitHub Changelog 2026-06-09). Pinning every consumer to `--allow-git` is exactly the adoption friction a standard must avoid, and npm's own guidance is "migrate to a proper registry." Advisory warnings already ship in npm 11.16+.
- **Vendored copy per tool.** Guarantees drift; "standard" becomes aspirational. Retained only as an escape hatch for fringe cases.

A published package (`bun add @byteowlz/design-system`) is zero-friction — resolved like any normal dep, bumped by Renovate/`bun update`, no `.npmrc`, no flags. The standard is brand-neutral and secret-free (oqto's palette is already public via this repo), so public npm visibility costs nothing.

## Why base24 + a closed role layer (not base16-only, not base32)

The chromatic base is **base24** (24 slots: 16 base16 + 8 darker-backgrounds and brights), chosen for ecosystem portability — the tinted-theming project ships 185 base24 + 320 base16 community schemes. **base16 schemes are supported** via a three-tier policy: prefer a same-name base24 twin if it exists (43 schemes), else *derive* the 8 missing slots (darken `base00`→`base10/11`; lighten `base08–0E`→`base12–17`), else (opt-in) duplicate. Derivation is *our* policy, not the tinted spec's — the spec defines slots and purpose only; base24 scheme authors hand-pick the extra 8. We derive because that is mechanically what a human port does, and because the alternative (aliasing `base11`→`base00`) collapses structural surfaces like the sidebar.

The engine never expands beyond 24 slots. A **closed abstract role layer** (16 roles, F1 minimal ramps: `foreground`/`muted-foreground`, `background`/`surface`/`surface-sunken`, `primary`/`secondary`/`muted`/`accent`/`success`/`warning`/`danger`/`info`, `border`/`ring`/`input`) sits above the slots and is the portable surface; membership is justified by universal screen-UI intent, not by echoing any framework's token names. Alpha/translucency is *derived* from slot values, not modeled as more slots. Effects (shadows/blur/motion) are deliberately an Identity concern (per-tool), never portable roles. Per-tool vocabulary (oqto's `terminal-*`, `code-*`, `chart-*`) and per-framework vocabulary (shadcn's `card`/`popover` split of `surface`, the `*-foreground` pairs) are non-portable extensions layered on the same slots. This is what makes "one mechanism, many looks" hold.

## Consequences

> **Superseded by ADR-0017** — the delivery consequences below (npm publish gating M1) no longer hold. The mechanism consequences (relocate `mini-apps/theming/` into the standard repo as `impls/shadcn-ts/`) **do** hold.

- `frontend/mini-apps/theming/` (the engine already built standalone) relocates to the standard repo's `impls/shadcn-ts/`; the rest of `mini-apps/` (SDK, workbench, apps) stays here as a consumer. *(Holds — see ADR-0017 for how the impl reaches oqto.)*
- The migration is a hard cutover (M1), gated on the npm package existing: `bun add @byteowlz/design-system`, swap `globals.css`'s `:root`/`.dark` blocks for `applyScheme(oqtoLight)` / `applyScheme(oqtoDark)`, delete the local copy. oqto's shadcn components read the same variable names, so no component rewrites. *(Superseded — M1 is gated on `byt design-system sync` existing, not on npm publish.)*
- Creating the `@byteowlz` npm org and first publish (`v0.1.0`) is a prerequisite, not part of this repo's code. *(Superseded — no longer a prerequisite.)*
- TUI/terminal tools are explicitly out of scope — tinted/tinty + base16-shell already own that domain. *(Holds.)*
