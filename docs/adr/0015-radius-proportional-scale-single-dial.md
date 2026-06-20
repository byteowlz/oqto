# Corner radius is a single dial driving a proportional categorical scale, sharp-at-0

Oqto's interface is sharp-cornered (`--radius-sm/md/lg/xl` and `--radius` are all `0` in `globals.css`) and will stay so as oqto's identity. Radius is exposed through the design-system standard as a **single tunable dial** that scales a **proportional categorical scale**, so a user (or another tool) can dial in rounded corners and every tier — including the innermost — rounds, while `dial = 0` keeps everything dead sharp.

## The scale (proportional)

The categorical tiers are multiplicative functions of the dial:

| tier | value |
|------|-------|
| `--radius-sm` | `calc(dial * 0.5)` |
| `--radius-md` | `calc(dial * 0.75)` |
| `--radius` / `--radius-lg` | `dial` |
| `--radius-xl` | `calc(dial * 1.25)` |

Components pick a tier by depth/role (innermost controls use `sm`, cards `lg`, full-bleed `xl`).

## Why proportional, not additive concentric

An earlier version of this decision chose strict concentric nesting (`childRadius = max(0, parent − inset)`). That was revised after the playground showed its failure mode: under strict concentric, the inner radius decays by the full padding at each level, so whenever the dial is smaller than the cumulative inset (the common case — paddings are ~24px), the innermost is starved to sharp. "The innermost never has a radius" at normal settings.

The two real requirements — **sharp-at-0** (oqto's identity) and **innermost rounds when the dial is up** — cannot both hold under additive concentric geometry (`outer − inner = inset` is non-negotiable). Proportional scaling satisfies both: every tier is a multiple of the dial, so all are `0` at `dial = 0`, and `sm = 0.5 × dial` is non-zero whenever the dial is. This matches how real UIs (Apple, shadcn) actually nest — harmonious proportional rounding, not strict shared-arc math.

## Exact concentric remains available (opt-in)

Components that explicitly want shared arc centers use `childRadius(parent, inset)` (decay inward) or `parentRadius(child, inset)` (grow outward, leaf-anchored). These are tools for deliberate nested showcases, not the default path.

## Consequences

- oqto's `globals.css` keeps `--radius*` at `0`; the standard's engine reads that as the dial value. No visual change until a user moves the dial.
- The standard's playground ships a radius slider verifying the scale live (every tier rounds, including innermost, from the same dial).
- Radius lives in the standard's `spec/radius.md` (target-agnostic) and is implemented in each framework impl (e.g. `impls/shadcn-ts/`); future `iced-rs`/`gpui-rs` impls implement the same scale natively.
