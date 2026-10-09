# Unified App Actions and host-owned action surfaces

## Status

Proposed (2026-09-04). Tracked by `oqto-y9jq`.

Extends [ADR-0038](0038-agent-built-apps-installation-binding-and-filesystem-discovery.md), [ADR-0041](0041-oqto-ui-container-compositor-and-agent-control.md), and [ADR-0044](0044-composable-platform-and-app-defined-agent-context.md). It does not make App-authored UI trusted and does not make MCP authoritative.

## Context

Oqto Apps must compose rather than behave as isolated websites. A generated image should be usable by an installed editor; a selected diagram should be exportable or attachable to Chat; the same semantic capability should be discoverable by an Agent, a command palette, a resource menu, a transformation pipeline, and eventually a native share flow.

Building one registry per surface would duplicate matching, authorization, lifecycle, and audit behavior. Letting Apps render Oqto permission or cross-App UI would let untrusted frames spoof trusted choices. Conversely, forcing every mature port such as Excalidraw to replace all domain-local menus with host chrome would destroy proven interaction behavior.

ADR-0044 already defines revision-bound semantic Actions for Context Providers. That action model needs a broader installed-App discovery and presentation layer without confusing human affordances, presentation-local commands, and server-affecting Actions.

## Proposal

### One Action Broker, multiple projections

An **App Action** is immutable Definition data describing a semantic operation. The Action Broker is the canonical authority for discovery, matching, authorization, invocation, lifecycle, audit, and results. Hosts project eligible Actions into surfaces; a surface never becomes a second execution authority.

Initial projections are:

- Oqto command palette;
- host-owned resource context menus;
- Agent action discovery and explicit invocation;
- visible linear Transformation Runs;
- App launch/open intents;
- later native share ingress, Shortcuts/App Intents adapters, and platform services.

An App advertises eligibility. Oqto decides whether, where, and how an Action appears. Apps cannot force placement, ranking, shortcuts, grants, or execution.

### Keep four concepts distinct

- **App Action:** typed domain behavior with bounded input/output schemas and an implementation binding.
- **Action Contribution:** declarative eligibility for one or more host surfaces.
- **Presentation Command:** requester-local behavior delivered to one exact active presentation, such as zooming or selecting a tool. It confers no server authority and is invalid when its declared presentation context no longer applies.
- **Transformation Run:** a durable, visible orchestration record containing ordered Action invocations and resource lineage. It is not hidden arbitrary code execution.

A command-palette row or menu item is an affordance which references one Action or Presentation Command. It is not itself an executable API.

### Definition-declared contract

The exact schema remains to be prototyped, but it must be bounded TOML plus JSON Schema, included in the Definition digest, and equivalent in shape to:

```toml
[[action]]
id = "canvas.image.annotate"
title = "Annotate in Canvas"
description = "Open an image in Canvas and create a non-destructive annotated derivative"
input_schema_file = "actions/canvas.image.annotate.input.json"
output_schema_file = "actions/canvas.image.annotate.output.json"
handler = { kind = "operation", id = "canvas.image.annotate" }
requires_user_activation = true

[[contribution]]
action = "canvas.image.annotate"
surfaces = ["resource-menu", "command-palette", "agent", "transformation"]

[contribution.accepts]
resource_kinds = ["workspace-file"]
media_types = ["image/*"]
cardinality = "one"

[contribution.when]
installation = "available"
instance = "active-or-creatable"
content = "open-or-launchable"
```

Supported handlers may include a pinned App operation, an App Instance contextual Action, a host-reviewed native Action, or an MCP tool adapter. Every handler delegates to the same Gate and cannot broaden its Action declaration.

The condition language is an intentionally small host-evaluated vocabulary, not JavaScript, Lua, CSS selectors, or a general expression evaluator. Initial facts may include Installation availability, Instance lifecycle, Content open/visible/focused state, active Presentation Context, declared topic availability, non-empty selection, resource kind/media type/cardinality, host capability availability, and exact grant state. Open, visible, focused, and selected remain distinct.

Apps may publish bounded ephemeral enabled/checked state only for command IDs declared by their pinned Definition. They cannot add runtime command identities, dynamic executable handlers, arbitrary markup, or model instructions.

### Resource subjects and launch intents

Cross-App matching uses typed subjects, never host paths, DOM nodes, Blob URLs, visible indexes, or App-generated bearer URLs. A resource subject carries a policy-checked opaque reference, media type, version, bounded label, owner/binding identity, and optional provenance.

Selecting a launch Action creates or focuses the target App Content and delivers a typed **App Launch Intent**. A Launch Intent names the Action, exact input subject references and versions, initiating Account/Work Session/Presentation Context, and a short-lived invocation identity. It is not a grant.

If the target App lacks access, Oqto offers a concise review for the narrowest useful authority. The preferred handoff is a one-resource read capability plus a separately bound output location. An editor creates a derivative by default; overwrite requires an explicit Action and expected source version.

The originating App cannot enumerate secret grant state, self-grant the target, forge an Installation, choose another Account, or transfer its own broad resource grant.

### Host-owned and App-owned menus coexist

Oqto owns shell menus, the command palette, cross-App Action presentation, permission review, and security-sensitive UI. A simple App such as Comfy Studio may delegate an entire resource menu to the Host through a transport-neutral `presentActions(subject, anchor)` interface.

A mature port such as Excalidraw may retain domain-local element menus inside its sandbox. It can request a host-owned `More in Oqto…` menu for the selected semantic subject. The App never renders Oqto permission UI or claims that an unverified local row is a trusted host Action.

Host menu presentation supports pointer context-menu activation, touch hold with movement cancellation, keyboard `Shift+F10`/menu key, accessible focus return, at least 44px touch targets, safe areas, and actual Container bounds. Pixel anchors are optional presentation hints, not semantic identity.

### Command palette contributions

The Host renders and searches all contributed rows. Titles are visibly namespaced when needed, for example `Canvas: Export selection as SVG`. Host commands outrank App contributions. Apps cannot reserve global shortcuts in v0; they may provide a bounded suggestion while Oqto/Account configuration owns bindings and conflicts.

A palette item may target:

- a focused Presentation Command;
- an Action against the current typed context/selection;
- a launchable installed App Action;
- a host-owned reveal/focus operation.

Presentation Commands are not automatically advertised to Agents. Agent-visible behavior requires an App Action with stable semantic input/output.

### Installed-only discovery in v0

Only installed, lifecycle-valid Apps contribute Actions. No marketplace search, implicit install, remote code acquisition, or recommendation appears in an Action surface in v0. An installed but ungranted Action may appear if selecting it leads to an explicit grant review; unavailable or unsupported Actions do not appear as if runnable.

### Linear Transformation Runs first

V0 supports ordered, non-destructive linear runs such as:

```text
Generate image → Remove background → Annotate → Upscale → Export
```

Each step pins Action ID, provider Installation/Definition/Instance as applicable, input resource versions, validated parameters, grant decision, status, timestamps, outputs, and lineage. Completed intermediate outputs survive later failure. Runs support cancel, retry from a failed step, and resume after reconnect. Agents may propose or invoke steps through the same Broker; they do not automate UI clicks.

Branching DAGs, speculative parallelism, implicit converter insertion, and cross-Account runs are deferred until linear traces prove their need.

### Native share ingress

Dynamically installed Apps cannot become independently signed iOS Share Extensions. A native Oqto Share Extension accepts platform items, stages them in a bounded host-owned inbox, creates scoped resource subjects, and asks the Action Broker for installed applicable Actions. Expensive execution is handed to the main Oqto host instead of running untrusted App code in the extension.

The same pattern can project to Android sharing, macOS Services, and future Shortcuts/App Intents adapters. Platform adapters consume the Action catalog; they do not become App authorities.

### Lifecycle and stale state

Definition change, supersession, grant revocation, suspension, uninstall, binding loss, Work Session end, or Presentation Context loss immediately invalidates affected discovery entries and pending invocation handles. Invocation rechecks identity, Definition digest, binding, grant, subject versions, expected context revision, and target Principal at the Gate.

A stale resource or context revision performs no mutation and returns current revision/version information. Menus and palette snapshots are disposable; selecting an old row never bypasses the live check.

## Consequences

- One semantic capability can appear in a context menu, command palette, Agent catalog, Transformation Run, and native ingress without duplicating execution code.
- Oqto needs an Action catalog/store, matcher, Broker/Gate integration, launch-intent model, host surface adapters, and conformance traces.
- The SDK needs presentation-menu requests and local Presentation Command delivery, but App frames receive no DOM or host registry access.
- One-resource handoff requires dynamic narrow resource bindings, expected-version I/O, and lifecycle invalidation.
- App-authored titles/descriptions remain untrusted data and never become instructions.

## Rejected alternatives

- **Separate menu, palette, Agent, and share registries:** duplicates matching and authorization and will drift.
- **Expose arbitrary App callbacks to host surfaces:** loads untrusted behavior into the decision path and is not portable.
- **Let source Apps enumerate and render cross-App actions:** enables spoofing and leaks host policy; use a host-owned menu request.
- **Make every App-local menu host-owned:** needlessly degrades mature ports and makes Oqto understand every domain command.
- **Treat command rows as Agent tools:** couples presentation wording/focus to durable semantics and bloats model tool catalogs.
- **Pass filesystem paths or bearer URLs between Apps:** bypasses resource identity, binding, and revocation.
- **Start with a general DAG engine:** increases failure/recovery semantics before linear composition is proven.

## Implementation slices

1. Rust Action catalog envelope, schema validator, immutable digest inclusion, and deterministic matcher.
2. Host-neutral Broker interface with in-memory conformance adapter and stale/grant/lifecycle tests.
3. OqtoUI command-palette projection for focused Presentation Commands and installed Actions.
4. SDK `presentActions` request plus host-owned resource menu on desktop/mobile/keyboard.
5. Scoped Launch Intents and one-resource handoff.
6. Comfy Studio image subject/action contributions and an Excalidraw-based Canvas fixture.
7. Linear Transformation Run store/executor and Agent/CLI adapter.
8. Native share-ingress adapter when a native host exists.

## Verification

1. One fixture Action projects consistently into menu, palette, Agent, and transformation catalogs without independent execution paths.
2. Matching rejects wrong media type, cardinality, owner, binding, lifecycle, focus, visibility, selection revision, and unsupported host capability.
3. A stale menu/palette selection performs no mutation after revocation, focus loss where required, Definition change, resource version change, or uninstall.
4. A source App cannot broaden a target App grant or pass an ungranted resource; one-file handoff exposes exactly one versioned resource.
5. Host-owned menus pass pointer, touch-hold, keyboard, focus-restoration, safe-area, and 390px viewport tests.
6. Excalidraw retains its domain-local menu while `More in Oqto…` remains visibly host-owned and invokes the same Action as the palette.
7. A Comfy → Canvas → upscale linear run preserves intermediate outputs, provenance, cancellation, retry, and reconnect recovery.
8. Agent and headless adapters discover/invoke identical Action IDs and schemas without receiving complete dynamic catalogs in every prompt.
9. An iOS fixture stages an inbound item and lists only installed applicable Actions; no App code executes in the Share Extension.

## Open questions before acceptance

- Exact schema subset for Action input/output and condition declarations.
- Whether installed-but-ungranted Actions appear directly or under one host-owned `Open with…` group.
- Retention and ownership of Transformation Runs.
- Which Presentation Commands merit optional user-owned shortcut suggestions.
