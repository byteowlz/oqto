# Offline-First Cross-Harness Visual Runtime

Status: PROPOSAL  
Date: 2026-03-29  
Owner: Oqto Platform

## 1) Problem Statement

Agents increasingly need to produce rich visual artifacts (architecture diagrams, plan audits, diff reviews, recap pages, dashboards, slide decks) that exceed terminal ergonomics.

Current approaches are fragmented and network-dependent:

- **Harness-specific implementations** (Pi-only extensions, one-off browser launchers)
- **CDN-dependent rendering** (Mermaid/Chart.js/morphdom loaded from remote origins)
- **Inconsistent UX** across Oqto and bare harness usage
- **Security/compliance concerns** from remote script execution
- **Offline fragility** in sandboxes/air-gapped environments

We need a single, harness-agnostic visual runtime that:

1. works **offline by default** (except LLM calls),
2. is reusable across harnesses,
3. preserves rich rendering capability,
4. keeps harness adapters thin.

---

## 2) Goals and Non-Goals

### Goals

1. **Cross-harness compatibility**
   - One visual artifact contract for Oqto + bare Pi + future harnesses.
2. **Offline-first execution**
   - No runtime CDN dependency for core visual capabilities.
3. **Rich feature parity**
   - Preserve high-quality diagrams, charts, decks, recaps, and interactive widgets.
4. **Strong security model**
   - Strict CSP, origin controls, and sanitization.
5. **Slim harness integrations**
   - Pi extension/skill should be a thin adapter over shared runtime.
6. **Deterministic behavior**
   - Versioned bundled assets and reproducible rendering output.

### Non-Goals

1. Replace A2UI (structured in-chat interaction).
2. Build a generic internet package loader at runtime.
3. Support arbitrary external script execution in strict mode.
4. Solve cloud deployment/distribution of visual artifacts in this phase (can be layered later).

---

## 3) Context and Prior Art

### Relevant projects

- **pi-generative-ui**
  - Strong streaming UX and native window integration.
  - Currently uses CDN and CDN-oriented generation guidance.
- **visual-explainer**
  - Strong design language for explainers/reviews/slides.
  - Currently uses CDN imports (Mermaid/ELK patterns, etc.).
- **oqto-serve proposal**
  - Good direction for in-platform visual hosting and panel integration.

### Observed gap

All three have useful pieces, but there is no shared runtime contract that guarantees offline execution and cross-harness portability.

---

## 4) High-Level Architecture

```
LLM (any harness)
   |
   | generates visual artifact spec / HTML fragment
   v
[Harness Adapter: Pi/Oqto/Other]  <-- thin bridge only
   |
   v
[Visual Runtime Core]
   - sanitizer/rewriter
   - local asset resolver
   - renderer bridge
   - strict CSP policy
   - event bridge
   |
   v
[Render Surface]
   - Oqto panel iframe
   - bare Pi native window/webview
   - browser fallback
```

### Principle

**All complexity lives in Visual Runtime Core.**  
Harness-specific layers only provide transport/session wiring and host integration.

---

## 5) Core Components

### 5.1 Visual Runtime Core (new shared module)

Responsibilities:

1. **Artifact normalization**
   - Accept HTML (or structured input) and normalize to runtime contract.
2. **Sanitization + rewrite pass**
   - Remove/replace remote script tags.
   - Map known libraries to local bundles.
3. **Asset injection**
   - Inject local runtime scripts/styles and theme bridge.
4. **CSP generation**
   - Enforce strict policy in offline mode.
5. **Render API**
   - `render({ html, title, mode, capabilities }) -> RenderHandle`.

### 5.2 Bundled Asset Pack (versioned)

Initial required bundle set:

- `morphdom`
- `mermaid`
- `@mermaid-js/layout-elk`
- `chart.js`
- optional phase 1: `three.js`
- optional phase 2: `d3`, `prism`/highlight tooling

All assets are local and version-pinned.

### 5.3 Runtime Contract (`window.VisualRuntime`)

Expose stable globals/utilities:

- `VisualRuntime.libs.mermaid`
- `VisualRuntime.libs.Chart`
- `VisualRuntime.libs.THREE` (if enabled)
- `VisualRuntime.theme.get()` / `subscribe()`
- `VisualRuntime.events.emit(type, payload)`

Generated artifacts should target this contract, not raw CDN imports.

### 5.4 Harness Adapters

#### Pi adapter (extension/skill)
- Tool registration (`visualize_read_me`, `show_widget` or equivalent).
- Forward generated artifact to runtime core.
- Receive render lifecycle callbacks.

#### Oqto adapter
- Integrate with serve/panel pipeline.
- Maintain session-scoped visual surfaces.
- Route theme + lifecycle events.

#### Future adapters
- Implement same minimal interface:
  - `startSurface`
  - `render`
  - `closeSurface`
  - `emitHostEvent`

---

## 6) Offline Modes and Policy

### Modes

1. **offline_strict (default)**
   - No external scripts/styles/fonts/images.
   - Remote imports rewritten or rejected.
2. **offline_prefer (transitional)**
   - Prefer local assets; allow explicitly allowlisted remote fetches with warnings.
3. **online_flexible (opt-in)**
   - Developer mode; remote imports allowed.

### CSP baseline for `offline_strict`

- `default-src 'none'`
- `script-src 'self' 'unsafe-inline'` (tighten to hashes/nonces in phase 2)
- `style-src 'self' 'unsafe-inline'`
- `img-src 'self' data: blob:`
- `font-src 'self' data:`
- `connect-src 'none'` (or localhost-only if required)
- `frame-ancestors` restricted to host surface

---

## 7) Richness Preservation Strategy

Concern: removing CDN reduces visual richness.

Answer: richness is preserved by shipping a curated local bundle.

### Feature mapping

- Architecture/flow/state/ER/class/mindmap/C4: **Mermaid + ELK**
- Dashboards/charts: **Chart.js**
- DOM streaming updates: **morphdom**
- 3D explainers: **Three.js** (bundled optional tier)
- Slide decks/recaps: CSS + local runtime utilities

No meaningful capability loss for common explainer use-cases.

---

## 8) Artifact Types and Contract

Supported artifact types:

1. **Widget/Single-page explainer**
2. **Review report** (diff/plan/fact-check)
3. **Recap page**
4. **Slide deck**
5. **Interactive dashboard**

Each artifact includes metadata:

```json
{
  "schemaVersion": "1",
  "title": "Auth flow review",
  "kind": "report",
  "requires": ["mermaid", "chartjs"],
  "theme": "auto",
  "content": "<section>...</section>"
}
```

Runtime injects required libraries from local bundle based on `requires`.

---

## 9) Security Model

### Threats

1. Remote script injection via model output.
2. Data exfiltration via fetch/XHR/websocket.
3. Host bridge abuse.
4. Persistent XSS across sessions.

### Controls

1. Sanitization pass blocks disallowed tags/attrs/protocols.
2. CSP blocks outbound connections in strict mode.
3. Host bridge is namespaced and capability-scoped.
4. Session-isolated render surfaces + no privileged DOM access.

---

## 10) Cross-Harness API Proposal

### 10.1 Core API

```ts
type RenderRequest = {
  artifact: VisualArtifact;
  mode: "offline_strict" | "offline_prefer" | "online_flexible";
  surface: { kind: "iframe" | "webview" | "browser"; id: string };
  context: { sessionId: string; workspace?: string };
};

type RenderResult = {
  surfaceId: string;
  revision: number;
  diagnostics: Array<{ level: "info" | "warn" | "error"; message: string }>;
};
```

### 10.2 Adapter interface

```ts
interface VisualHarnessAdapter {
  ensureSurface(input: { sessionId: string; title?: string }): Promise<{ surfaceId: string }>;
  render(req: RenderRequest): Promise<RenderResult>;
  closeSurface(surfaceId: string): Promise<void>;
  onHostEvent(cb: (e: HostEvent) => void): () => void;
}
```

---

## 11) Migration Plan

### Phase 0: Spec + inventory
- Define runtime contract and strict mode behavior.
- Inventory all CDN dependencies in existing skills/extensions.

### Phase 1: Runtime core + Pi adapter
- Build core sanitizer/injector/asset resolver.
- Add local bundles (Mermaid/ELK/Chart/morphdom).
- Update Pi extension to use core.

### Phase 2: Oqto integration
- Wire runtime into Oqto serve/panel path.
- Add lifecycle events and per-session render surfaces.

### Phase 3: Skill/reference refactor
- Rewrite visual generation guidelines to local-bundle contract.
- Add “no CDN in strict mode” test fixtures.

### Phase 4: Hardening + performance
- CSP hardening with nonces/hashes.
- Bundle split by capability.
- cold/warm render benchmarks.

---

## 12) Testing and Validation

### Functional tests
1. Render each artifact type fully offline (network denied except LLM).
2. Ensure diagrams/charts/decks render correctly in:
   - Oqto panel,
   - bare Pi window.
3. Ensure strict mode rejects or rewrites remote imports.

### Security tests
1. Injection attempts via script tags and event handlers.
2. Fetch/XHR/websocket attempts in strict mode.
3. Bridge misuse attempts.

### Performance tests
1. Cold start render latency.
2. Incremental update latency.
3. Memory footprint by artifact complexity.

---

## 13) Acceptance Criteria

1. **No runtime CDN dependency in `offline_strict`.**
2. Same artifact spec renders in Oqto and bare Pi with no harness-specific content changes.
3. Mermaid+ELK+Chart render locally with parity to current visual quality.
4. Three.js optional local bundle available behind feature flag.
5. Sanitizer + CSP block remote script execution and outbound network in strict mode.
6. Pi adapter remains thin (no duplicated runtime logic).

---

## 14) Risks and Mitigations

1. **Bundle bloat**
   - Mitigate with capability-based lazy bundles.
2. **Model keeps emitting CDN snippets**
   - Mitigate with rewrite pass + updated prompting + lint checks.
3. **Divergent host surfaces**
   - Mitigate with strict runtime contract and adapter conformance tests.
4. **Mermaid/ELK version drift**
   - Mitigate with pinned versions + compatibility matrix.

---

## 15) Operational Considerations

1. Version visual runtime independently (`visual-runtime@x.y.z`).
2. Expose diagnostics panel/log lines for rewrite and CSP violations.
3. Add telemetry counters (local-only where required):
   - artifact kind,
   - render time,
   - rewrite count,
   - blocked-remote-attempt count.

---

## 16) Open Questions

1. Should Three.js be in baseline bundle or optional capability pack?
2. Do we include local font packs by default or rely on system stacks?
3. How strict should CSP be in development mode?
4. Should artifact schema include explicit accessibility requirements?

---

## 17) Recommended Initial Decision Set

1. Adopt **offline_strict default**.
2. Build **shared Visual Runtime Core** first, then adapters.
3. Ship baseline local bundle: morphdom, mermaid, ELK, chart.js.
4. Keep Three.js as optional capability in phase 1.5.
5. Refactor visual skills/prompts to `window.VisualRuntime` contract.

---

## 18) References

- `https://github.com/nicobailon/visual-explainer`
- `https://github.com/nicobailon/pi-design-deck`
- `https://github.com/Michaelliv/pi-generative-ui`
- `docs/design/oqto-serve.md`
