# Oqto Customizations: Lua-configurable UI on first-party primitives

## Status

Proposed (2026-08-19). Tracked by `oqto-nq3b`. Companions: [ADR-0037](0037-oqto-ui-portable-rearrangeable-views.md) (portable Views, layout tree, fidelity), [ADR-0038](0038-agent-built-apps-installation-binding-and-filesystem-discovery.md) (Apps plane), [ADR-0036](0036-interactive-output-parts-declarative-first.md) (declarative-first precedent), [ADR-0035](0035-unified-egress-tiers-traceability-pluggable-enforcers.md) (enforcement/grant precedent).

Design research: `docs/frontend/oqto-ui-corner-mode-ux-research.md` and the corner-mode probes under `artifacts/oqto-ui/corner-mode-concepts/` (`oqto-m5sp`).

## Context

Oqto's extension model already names three planes: **Apps** manipulate work/content, **Oqto Customizations** alter a user's host UI/workflow, and **Runtime Add-ons** alter the harness. ADR-0038 designed the Apps plane. This ADR designs the Customizations plane.

The goal is Neovim-grade configurability: users (and their Agents, through ordinary files) shape the cockpit — layouts including multi-Chat grids, keymaps and gesture bindings, corner slots, radial/carousel menus, pickers, theme roles — without the trusted kernel giving up identity, authorization, grants, Session authority, sandboxing, or recovery. The interaction probes (`oqto-m5sp`) produced a concrete vocabulary that any such system must be able to express, and the frontend must not become a second, incompatible way to build it.

Constraints inherited from standing decisions:

- Agent-facing capabilities are host-neutral (web, TUI, future GPUI/iOS render the same contracts).
- User-owned configuration is preserve-first with backup/diff/merge semantics.
- Unsupported configuration values fail closed.
- No unsandboxed third-party code in host processes.
- Placement never changes ownership, binding, or grants (ADR-0037).
- Simple APIs before invented protocols; extension points require a proven second producer.

## Decision

### One dogfooded contract: the first-party UI is built on the public primitives

OqtoUI's own shell consumes the same primitives that customizations configure. There is no private layout, menu, keymap, or picker machinery that config cannot reach, and no config capability the shell does not itself exercise. The shipped experience is a **preset** — a readable, forkable configuration file — not a privileged code path.

**Proof obligation:** the corner-mode interaction set (corner slots, tap/hold vocabulary, quarter-arc radial, hold carousels, MRU fan, navigator, quick-scroll rail) must be expressible as a preset on these primitives. Classic split, corner mode, and big-picture ship as three presets. If a probe interaction cannot be expressed, the primitive inventory — not the preset — is wrong.

### Three layers: schemas are the contract, Lua is the ergonomics, WASM is the substrate

**Layer 1 — Declarative schemas (the contract).** The configuration contract is versioned, JSON-serializable data: layout trees, keymaps, bindings, menu definitions, picker settings, theme role mappings. Hosts interpret this data; producers are interchangeable (Lua, a settings UI, an Agent, hand-written JSON). Schemas are Rust-canonical with generated TypeScript types, versioned with explicit migrations. Validation failures are doctor findings; an invalid layer is dropped while lower layers still apply.

**Layer 2 — Lua (the blessed scripting layer).** A sandboxed Lua 5.4 VM evaluates user configuration into Layer-1 data plus event-driven hooks: wasmoon (Lua-on-WASM) in browser hosts, mlua in Rust hosts, one conformance suite guaranteeing identical semantics. The full API ships LuaCATS annotations for editor tooling. Lua is chosen over embedded JavaScript/TypeScript because it is sandboxable by construction (zero ambient authority; its world is exactly the injected API), an order of magnitude smaller, culturally aligned with the target audience, and runs identically in web and native hosts.

**Layer 3 — WASM (substrate and deferred escape hatch).** WASM is how Lua is sandboxed in the browser, not a config format. A compiled-component interface for hooks/matchers is documented as a future extension point and deliberately **not built** until a concrete second producer exists. Anything heavy enough to want compiled code is an App, not a customization.

### The hard rule for config code

> Lua produces data and calls semantic actions. It never renders, never fetches, never touches stores, never sees the DOM, and never receives another user's or Session's data beyond what its capability grants expose.

Hooks are event-driven with explicit fuel/time/memory budgets. A hook that exceeds budget or errors is suspended and reported; the shell continues on last-known-good data. Config errors can degrade a cockpit, never corrupt state: all mutation flows through the same typed Actions any UI click uses, subject to the same authorization.

### Primitive inventory

| Primitive | Contract |
| --- | --- |
| **Action** | Named semantic operation (`session.switch`, `chat.send`, `tool.open`) with typed parameters; host-neutral; authorization enforced at execution, never at binding. |
| **View** | ADR-0037 View instances, including App Views. Config references Views by identity; binding stays explicit and placement-independent. |
| **Layout** | ADR-0037's versioned tree, extended with grid nodes. Multiple Chat Views may be open simultaneously; each leaf carries its own explicit Session binding. |
| **Surface/Slot** | Named mount points: corner slots, edge strips, sheets, status segments, rail. Slots accept Menu or View references. |
| **Menu** | Radial (quarter-arc at corners, ≤4 items, dead-zone cancel), carousel, fan, list. Items reference Actions; presentation is data. |
| **Binding** | Input chords: keys, tap/hold/blank-release gestures (320 ms dwell, constant), controller mappings. Bindings reference Actions or Menus. |
| **Provider** | Streaming, capability-gated list source (sessions, workdirs, files, message FTS, App-contributed, sandbox-tool-backed). Declared in a queryable catalog with versions. |
| **Matcher** | Built-in fuzzy engines (subsequence, fzf-style, frecency-weighted) selected and parameterized as data; large corpora match provider-side; conformance-tested across hosts. |
| **Theme** | Existing Base24 role mapping; palettes remain immutable, roles are configurable data. |

### Configuration layering and loading

Layers, lowest to highest precedence:

1. **Dist defaults** — shipped presets, immutable, versioned with the release.
2. **Deployment** — admin policy for the instance.
3. **User** — `~/.config/oqto/` (`init.lua` + modules); user-owned, preserve-first, synced by the existing user-config mechanism.
4. **Workspace** — `.oqto/ui.lua` in a work directory; because workspace files may be Agent-written, this layer is **grant-gated**: inert until the user approves it (direnv-style allow), re-approval required on content change.
5. **Ephemeral** — URL/session state.

The browser never reads config files. The backend config service evaluates layers, serves merged Layer-1 data, and keeps every layer versioned, diffable, and revertible. A failing layer drops out with a doctor finding; the shell always boots from at least the dist defaults.

### Provider catalog and the drift invariant

Native tools backing Providers (ripgrep-class search, indexers) run inside workdir sandboxes or the backend — never on the client. Multi-machine and multi-tenant variance is therefore confined to the **catalog**:

> Config binds semantic Provider IDs; the catalog resolves availability per deployment/runner/workspace; absence is visible and diffable, never discovered at use-time by failure.

- Engine behavior (matchers, ranking, picker semantics, schemas) ships with the release and is conformance-tested: same release, same behavior, everywhere.
- The **platform baseline** (sessions, workdirs, file listing, oqto-log FTS search) is a release contract, always present; shipped presets use only the baseline and are drift-free by construction.
- Beyond baseline, pickers declare fallback chains (`prefer`/`fallback`); missing providers degrade declaratively and surface as named doctor findings.
- Workspaces may pin required providers/versions in `.oqto/`; reconciliation (including Agent-driven sandbox provisioning) flows through the normal grant paths.
- Tenant policy removes providers through the same catalog, so denial is explained, not mysterious.

### Relationship to Apps and the kernel

- **Apps provide content** (Views, Actions, Providers) under ADR-0038 sandboxing and grants. **Customizations arrange and bind** — they may reference App contributions but binding never confers authority.
- **The kernel is not configurable:** identity, authorization, grants, Session identity/authority, sandbox boundaries, recovery, and audit are outside the config plane. No Lua hook observes another tenant, another user, or Session content beyond its granted capabilities.
- Provider result streaming uses the typed ephemeral-event path; search results are invalidation-class data, never durable truth (bus-replacement direction, `oqto-07q8`).

## Consequences

- The shell refactors toward interpreting Layer-1 data (slots, bindings, menus, layout) instead of hardcoding composition; this begins as the corner-mode preset is productized, and the gallery probes become executable acceptance tests for the primitives.
- New mandatory infrastructure: schema versioning + migrations, wasmoon/mlua conformance suite, hook budgets, config doctor, provider catalog endpoint. These are acceptance criteria for the first shipped customization, not later hardening.
- Multi-Chat grids force timeline virtualization and per-View subscriptions before the grid preset ships.
- The config plane adds a supported surface with real maintenance cost; the compensation is deleting bespoke one-off UI options (each becomes data on a primitive) and gaining preset-based UX experimentation without frontend releases.
- Risks accepted: Lua's dynamic typing (mitigated by LuaCATS + load-time schema validation), the temptation to grow hooks into an app platform (mitigated by the hard rule and budgets), and deferred compiled-component support (revisited when a second producer exists).

## Verification

- Conformance: identical config in wasmoon and mlua yields identical Layer-1 data and hook outcomes (golden tests).
- Proof obligation: corner-mode, classic, and big-picture presets expressed purely as configuration, driving the same shell.
- Fail-closed: corrupted layer at every level boots the shell on remaining layers with doctor findings; kernel surfaces unaffected.
- Drift: removing a non-baseline provider degrades the bound picker along its declared fallback and emits the named finding; baseline-only presets show zero behavioral diff across two deployments of the same release.
