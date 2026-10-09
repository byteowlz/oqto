# One App contract (`OqtoHost`); `apphost` is a transport, not an API; Surfaces stay bundled

Status: proposed (2026-07-26), refined by accepted [ADR-0037](0037-oqto-ui-portable-rearrangeable-views.md) and [ADR-0038](0038-agent-built-apps-installation-binding-and-filesystem-discovery.md). **Supersedes the `window.apphost` API and the `--app-*` CSS token contract defined in epic `oqto-dbbw`** (that epic is closed, but its code is live — see "Unretired runtime" below). Its `OqtoHost`/Bridge/Gate capability direction survives; ADR-0038 sharpens Artifact into Definition/Installation/Instance/binding/View and adds portable declarative and sandboxed-web presentations, while ADR-0037 makes Views the OqtoUI composition primitive. [ADR-0031](0031-session-centric-workbench-shell.md) superseded this document's bundled-Surface list and `lib/app-registry.ts` survival decision. Extends [ADR-0019](0019-container-per-workspace-placement.md) with the App Sidecar concept. Depends on [ADR-0017](0017-design-system-vendored-via-byt-not-npm-publish.md) M1 for a single palette source. Names the consumer that [ADR-0012](0012-oqto-bus-is-the-in-process-app-ui-fabric.md) makes the bus's survival condition.

Oqto grew **three** app runtimes independently, each answering "how does a non-shell UI run inside Oqto" differently. None is wrong; together they are unbuildable-on. Every further app feature — standalone tool UIs, agent-driven apps, sharing, admin promotion — would otherwise land as a fourth.

| Runtime | Contract | Reach |
|---|---|---|
| `AppView.tsx` + `visual-runtime/` | `window.apphost`: stringly `postMessage`, srcdoc iframe, hardcoded light/dark `--app-*` hex maps, `.oqto/app-state/` persistence | live; agent-written workspace HTML files |
| `mini-apps/sdk` | `OqtoHost`: typed capability objects (`files`/`kv`/`notifications`/`theme`/`user`), declared capability keys, mock host for standalone use | live; **workbench only** |
| `lib/app-registry.ts` | `AppDefinition` holding a bundled React `component`, plus `routes` and role gating | live; Oqto's own top-level sections |

## Decisions

1. **`OqtoHost` is the App contract. `apphost`/`postMessage` is demoted to one *transport* of it.** `sdk/host.ts` was built for exactly this — its doc comment already commits to a promise-based, serializable surface ("refs, Blobs, plain values — never raw handles or DOM nodes") *so that* the same interface can be re-implemented over a postMessage/iframe bridge with no app-side changes. `AppView` keeps its iframe plumbing and loses its private API. There is one app-facing surface, expressed once, in TypeScript.

2. **Bundled Surfaces are retained as-is, deliberately.** `dashboard`, `sessions`, `agents`, `settings`, `admin` stay build-time-linked in `lib/app-registry` with no capability boundary. They *are* the trusted shell; wrapping them in a permission surface would be pure overhead with no security gain. `lib/app-registry` is not redundant with the SDK — it is a **navigation registry**, a different concept, and it survives. The single exception is `sldr`, an external tool with its own repo and release cadence that became a Surface only for want of another mechanism; it is the out-migration candidate and the proof case.

3. **The agent capability is mediated by the runner, not published directly onto oqto-bus.** App actions and agent-initiated calls share one policy enforcement point (the Gate), so per-workspace permissions cannot be bypassed by choosing a different path. ADR-0012's awaited consumer therefore arrives *through the runner*; the bus remains the fabric beneath it, not a parallel authority.

4. **Capabilities are requested by the App and granted by a principal; never self-granted.** A manifest declares what it wants; the installing Account (or an admin, at promotion) grants against *their own* mounts. Grants are keyed on `(installed app, principal, mount)` and enforced at the Gate — never on an App's own assertions.

5. **App palettes derive from the design-system roles.** No runtime may carry its own hex map. This makes ADR-0017 M1 a prerequisite rather than a parallel cleanup: while the shell renders from static shadcn variables and `AppView` from hardcoded hexes, `host.theme.getScheme()` reports a scheme the surrounding UI does not actually use, and no amount of app-side work makes an embedded App look native.

## Unretired runtime

Epic `oqto-dbbw` is **closed, but `AppView.tsx` and `visual-runtime/` are running code**. It must be treated as an active runtime being superseded, not as dead code to delete on sight: agent-written workspace HTML files depend on `window.apphost` and on `.oqto/app-state/` today. Demotion means the bridge keeps working while the API it exposes becomes a rendering of `OqtoHost` — not a flag day.

## Three layers that must not fuse

The recurring confusion in this area is treating an App's *identity* as its *placement*. They are independent:

- **Artifact** — code plus manifest, versioned and content-addressed. Portable, shareable, promotable.
- **Instance** — a running copy, isolated.
- **Data binding** — which mounts it may touch, granted per principal and chosen at launch.

ADR-0020 §7 (work directories are a typed mount source, not a host path) is the binding seam. Because these are separate, "a general-purpose tool over files the user selects" and "isolated per Workspace" are *not* in tension — which is what makes sharing and promotion tractable at all.

## Consequences

- The SDK must become distributable before any standalone tool UI can exist; today it sits behind `@/mini-apps/*` aliases inside Oqto's frontend build, so no external repo can import it. Channel is `byt` distribute-down sync, not npm: ADR-0017's npm trigger (a non-byteowlz-internal consumer, or one that cannot run `byt`) is unmet.
- `mini-apps/shell/OqtoAppShell.tsx` chrome belongs in the design-system (`spec/slots.md` covers layout), so a standalone tool UI looks native without importing anything Oqto-specific. The host/capability contract does **not** belong there: design-system is a visual contract with potential external consumers, and welding Oqto's permission model to it would fuse two unrelated release cadences.
- `CONTEXT.md` gains **Surface**, **App**, **Host**, **Bridge**, **Gate**, **App Sidecar**. Three registries all saying "app" is how this divergence went unnoticed.
- Pi has no MCP and no tool wiring, so the agent reaches App actions codemode-style (`search` → `describe` → call) rather than as N flat tools. This is not a workaround: the action surface is *dynamic per workspace*, and an MCP client would need a restart whenever an App boots mid-session, whereas prompt cost here stays constant as Apps are added.
- **Pod shared netns is not a trust boundary.** ADR-0019 makes a Workspace a Pod, so agent code and any App Sidecar share loopback; the Gate must therefore be enforced by uid separation inside the Pod (App socket `0600`, gate-uid only; Gate socket `0660`, shared with the agent uid), never by network reachability. Same class of exposure as `oqto-5spg`.
- Promoted App UIs execute in *other* users' browsers, so Apps must be served from a distinct or opaque origin with a per-instance grant token from the gateway — never the Oqto origin riding the viewer's session cookies. Without this, "admin promotes an App" is a path from one agent's generated JS to every Account's session.
- **Capability combinations carry the risk, not individual capabilities.** `files` alone is safe; `egress` alone is safe; `files` + `egress` is an exfiltration channel. Grant UI surfaces the combination; egress is audited and rate-limited per `(app, principal, connection)`.
- An App can never hold a credential (its JS is readable, and for a promoted App that JS was agent-written). Egress is a host capability performed server-side against admin-curated connections, so an App requests a declared connection and cannot invent an integration.

## Deliberately not decided here

Whether agent-authored Apps may run server-side. That turns on measured sidecar cost (`oqto-gqgh.6`) and on the trust escalation involved — agent-authored code executing in *another tenant's* Pod is a real step up from browser-only execution, independent of price. Until then `serves_http` is assumed off for agent-authored Apps and available to admin-reviewed ones. Also deferred: the manifest format, the catalog, promotion mechanics, and the `sldr` migration — all of which depend on this ADR landing first.
