# Oqto Customizations: Lua-configurable UI on first-party primitives

## Status

Proposed (2026-08-19). Tracked by `oqto-nq3b`. Companions: [ADR-0037](0037-oqto-ui-portable-rearrangeable-views.md) (portable presentation and fidelity), [ADR-0041](0041-oqto-ui-container-compositor-and-agent-control.md) (Container compositor and layout transactions), [ADR-0038](0038-agent-built-apps-installation-binding-and-filesystem-discovery.md) (Apps plane), [ADR-0036](0036-interactive-output-parts-declarative-first.md) (declarative-first precedent), [ADR-0035](0035-unified-egress-tiers-traceability-pluggable-enforcers.md) (enforcement/grant precedent).

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
| **Container Content** | ADR-0041 Content instances, including first-party and App presentations. Config references Content by identity; binding stays explicit and placement-independent. |
| **Layout** | ADR-0041's versioned Container compositor and semantic Grid. Multiple Chat Content items may be open simultaneously; each keeps its own explicit Session binding. |
| **Surface/Slot** | Named mount points: corner slots, edge strips, sheets, status segments, rail. Slots accept Menu or Container Content references. |
| **Menu** | Radial (quarter-arc at corners, ≤4 items, dead-zone cancel), carousel, fan, list. Items reference Actions; presentation is data. |
| **Binding** | Input chords: keys, tap/hold/blank-release gestures (320 ms dwell, constant), controller mappings. Bindings reference Actions or Menus. |
| **Provider** | Streaming, capability-gated list source (sessions, workdirs, files, message FTS, App-contributed, sandbox-tool-backed). Declared in a queryable catalog with versions. |
| **Matcher** | Built-in fuzzy engines (subsequence, fzf-style, frecency-weighted) selected and parameterized as data; large corpora match provider-side; conformance-tested across hosts. |
| **Theme/Appearance** | Base24 semantic roles plus typed geometry, Paint, and Effect recipes. Configuration targets named semantic surfaces, never CSS selectors or renderer internals. |

### Appearance is typed and capability-adaptive, not restricted to flat color

Customizations may substantially change OqtoUI's appearance without injecting CSS. The schema addresses **semantic targets** (for example `surface.chrome`, `surface.content`, `view.chat.user-message`, `menu.radial`, `focus.indicator`) and assigns target-neutral recipes:

- **Geometry:** the single proportional radius dial from [ADR-0015](0015-radius-proportional-scale-single-dial.md), density/spacing scale, border width and emphasis, and typography roles.
- **Paint:** a semantic solid color or linear/radial gradient with semantic color-role stops, direction/origin, and bounded opacity. Stops reference palette roles or validated theme colors rather than CSS strings.
- **Effects:** named elevation levels or a bounded shadow recipe, surface/content opacity, backdrop or content blur, and bounded saturation/contrast where supported. Effect stacks have schema limits; they cannot contain arbitrary renderer expressions.
- **States:** recipes for focus, selection, hover/press, disabled, warning, and destructive states. No function may exist only on hover.

This is feasible in the web host (CSS gradients, shadows, opacity, `filter`, and `backdrop-filter`) and in modern native GPU renderers. It is not uniformly representable in every host, especially a terminal. Therefore each host publishes **appearance capabilities**, and every rich recipe has a deterministic fallback encoded in the schema: gradient → designated solid role; multiple/custom shadow → nearest elevation level or border emphasis; backdrop blur → opaque/translucent surface paint; unsupported filters → identity. A preset may require only the portable baseline or declare richer optional effects. Unsupported optional effects do not break the UI; preview and doctor report the fallback.

Accessibility and deployment policy resolve after user appearance: forced-colors/high-contrast may replace Paint recipes, contrast floors override unsafe text/background combinations, reduced-transparency disables blur/translucency, and reduced-motion remains authoritative. This is an intentional kernel guardrail, not config precedence.

Apps may style content inside their own sandboxed presentation. They cannot inject selectors or effects into OqtoUI surfaces. If a visual concept cannot be represented by the typed vocabulary, its author proposes a cross-host Appearance primitive and fallback; raw CSS is not the escape hatch.

### External theme ecosystems are data sources; Omarchy is the first compatibility fixture

Oqto may acquire palettes and Appearance presets from external theme ecosystems through explicit, versioned **input adapters**. Imported themes are inert Customization/Appearance packages, never Apps: importing one grants no filesystem, network, Session, Agent, or execution capability. Importers parse and validate declared data; they never execute a theme's scripts, Lua, CSS, templates, installer, or renderer-specific configuration.

[Omarchy 4/Quattro](https://github.com/basecamp/omarchy/tree/quattro) is the first required adapter and compatibility fixture. Its canonical semantic palette maps deterministically to Base24: four background/selection/muted slots and four foreground slots map to `base00..base07`; red/orange/yellow/green/cyan/blue/magenta/brown map to `base08..base0F`; dark/darker backgrounds plus six bright accents map to `base10..base17`. Omarchy's independent `accent` and `mode` remain explicit semantic metadata instead of being guessed from a fixed slot. Legacy Omarchy themes using `color0..color15` are accepted through a separately versioned ANSI compatibility resolver; ANSI numbering must never be mislabeled as Base16/Base24.

Supported acquisition paths are:

1. **Local discovery:** read installed and active Omarchy themes without modifying Omarchy-owned or user-owned files.
2. **Pinned Git import:** fetch a theme repository, pin the resolved commit/content digest, and import only allowlisted data files.
3. **Gallery catalog:** present generated OqtoUI previews, source/author/license, compatibility diagnostics, accessibility checks, imported fields/fallbacks, and an explicit “install as user Appearance” action.
4. **Optional active-theme synchronization:** follow the locally active Omarchy theme only after opt-in; every update creates a resolved, versioned Appearance with source provenance and remains independently revertible.

The importer preserves source URL/path, author/license metadata, source format/version, content digest, original palette, translated Base24 slots, semantic role bindings, diagnostics, and applied fallbacks. It may safely translate a documented subset of Omarchy shell data (semantic surface colors, opacity, border treatment, spacing, typography) into typed Appearance recipes. Hyprland Lua, arbitrary templates/CSS, wallpapers, icons, fonts, keyboard RGB, and application-specific files are not portable appearance data and are not executed or installed by this adapter. Asset acquisition requires separate explicit licensing and installation decisions.

This establishes an adapter pattern, not an Omarchy dependency and not a second palette system. Other ecosystems must translate into the same Layer-1 Appearance schema and meet the same provenance, security, and fallback requirements.

### Configuration layering and loading

Layers, lowest to highest precedence:

1. **Dist defaults** — shipped presets, immutable, versioned with the release.
2. **Deployment** — admin policy for the instance.
3. **User** — `~/.config/oqto/` (`init.lua` + modules); user-owned, preserve-first, synced by the existing user-config mechanism.
4. **Workspace** — `.oqto/ui.lua` in a work directory; because workspace files may be Agent-written, this layer is **grant-gated**: inert until the user approves it (direnv-style allow), re-approval required on content change.
5. **Ephemeral** — URL/session state.

The browser never reads config files. The backend config service evaluates layers, serves merged Layer-1 data, and keeps every layer versioned, diffable, and revertible. A failing layer drops out with a doctor finding; the shell always boots from at least the dist defaults.

### Evaluation, resolution, and provenance are distinct

There is no context-free “the Oqto config.” A value may come from one source layer and resolve differently by Account, Workspace/work directory, host capabilities, accessibility policy, or ephemeral state. Tooling therefore distinguishes:

- **`oqto config eval <file>`** evaluates one explicit Lua file in isolation and prints normalized Layer-1 data plus diagnostics. `--layer dist|deployment|user|workspace|ephemeral` selects one named source from an explicit context. It does not merge other layers.
- **`oqto config resolve`** prints the effective merged config for an explicit target context. It accepts deployment, Account, Workspace/work-directory, preset, host, and accessibility/capability inputs. Interactive use may infer Account and work directory from authenticated CLI/cwd context, but output always records those resolved IDs; automation must pass them explicitly.
- **`oqto config explain <json-pointer>`** reports the winning value, every overridden candidate, source path/content digest, schema migration, host fallback, and policy/accessibility override for one effective field.
- **`oqto config diff`** compares source layers or two resolved contexts without executing actions.

All commands default to structured JSON on non-TTY output and have a human-readable TTY rendering. Secret values are never embedded in configuration output. Evaluation is side-effect-free: hooks are compiled/validated but not fired, and Actions are represented symbolically. The resolved artifact carries its schema version, source digests, target IDs, host capability profile, diagnostics, and per-field provenance, making it a reproducible verification surface for humans and Agents.

### Provider catalog and the drift invariant

Native tools backing Providers (ripgrep-class search, indexers) run inside workdir sandboxes or the backend — never on the client. Multi-machine and multi-tenant variance is therefore confined to the **catalog**:

> Config binds semantic Provider IDs; the catalog resolves availability per deployment/runner/workspace; absence is visible and diffable, never discovered at use-time by failure.

- Engine behavior (matchers, ranking, picker semantics, schemas) ships with the release and is conformance-tested: same release, same behavior, everywhere.
- The **platform baseline** (sessions, workdirs, file listing, oqto-log FTS search) is a release contract, always present; shipped presets use only the baseline and are drift-free by construction.
- Beyond baseline, pickers declare fallback chains (`prefer`/`fallback`); missing providers degrade declaratively and surface as named doctor findings.
- Workspaces may pin required providers/versions in `.oqto/`; reconciliation (including Agent-driven sandbox provisioning) flows through the normal grant paths.
- Tenant policy removes providers through the same catalog, so denial is explained, not mysterious.

### Relationship to Apps and the kernel

- **Apps provide content** (App Content presentations, Actions, Providers) under ADR-0038 sandboxing and grants. **Customizations arrange and bind** — they may reference App contributions but binding never confers authority.
- **The kernel is not configurable:** identity, authorization, grants, Session identity/authority, sandbox boundaries, recovery, and audit are outside the config plane. No Lua hook observes another tenant, another user, or Session content beyond its granted capabilities.
- Provider result streaming uses the typed ephemeral-event path; search results are invalidation-class data, never durable truth (bus-replacement direction, `oqto-07q8`).

## Consequences

- The shell refactors toward interpreting Layer-1 data (slots, bindings, menus, layout) instead of hardcoding composition; this begins as the corner-mode preset is productized, and the gallery probes become executable acceptance tests for the primitives.
- New mandatory infrastructure: schema versioning + migrations, wasmoon/mlua conformance suite, hook budgets, config doctor, provider catalog endpoint. These are acceptance criteria for the first shipped customization, not later hardening.
- Multi-Chat grids force timeline virtualization and per-Content subscriptions before the grid preset ships.
- The config plane adds a supported surface with real maintenance cost; the compensation is deleting bespoke one-off UI options (each becomes data on a primitive) and gaining preset-based UX experimentation without frontend releases.
- Omarchy immediately supplies a broad, curated palette ecosystem and a real compatibility corpus, while Oqto owns translation, previews, accessibility validation, provenance, and deterministic host fallbacks.
- Risks accepted: Lua's dynamic typing (mitigated by LuaCATS + load-time schema validation), the temptation to grow hooks into an app platform (mitigated by the hard rule and budgets), and deferred compiled-component support (revisited when a second producer exists).

## Verification

- Conformance: identical config in wasmoon and mlua yields identical Layer-1 data and hook outcomes (golden tests).
- Inspection: `config eval` of each source layer and `config resolve` of a pinned context reproduce normalized golden artifacts; `config explain` attributes an overridden and a host-fallback value to exact source digests.
- Proof obligation: corner-mode, classic, and big-picture presets expressed purely as configuration, driving the same shell.
- Appearance: Paint/Effect golden scenes render gradients, shadows, opacity, blur, radius, and focus states in the web reference host; a baseline-only host applies the specified fallbacks; forced-colors and reduced-transparency preserve readable contrast and operation.
- Omarchy compatibility: every bundled Quattro semantic palette imports with all 24 mapped colors plus explicit accent/mode and round-trips through the normalized Oqto artifact without loss; representative legacy ANSI themes resolve deterministically; malicious values and executable/theme-specific files are rejected or ignored with diagnostics.
- Acquisition: local, pinned-Git, and Gallery imports produce identical normalized artifacts for the same source digest; active-theme synchronization is opt-in, versioned, and revertible; license/provenance remain visible through installation and export.
- Fail-closed: corrupted layer at every level boots the shell on remaining layers with doctor findings; kernel surfaces unaffected.
- Drift: removing a non-baseline provider degrades the bound picker along its declared fallback and emits the named finding; baseline-only presets show zero behavioral diff across two deployments of the same release.
