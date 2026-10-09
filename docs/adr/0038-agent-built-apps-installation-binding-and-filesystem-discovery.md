# Agent-built Apps: portable presentations, explicit binding, filesystem discovery without path authority

## Status

Accepted (2026-08-09). Tracked by `oqto-171p`. Companion UI-host decisions: [ADR-0037](0037-oqto-ui-portable-rearrangeable-views.md) and [ADR-0041](0041-oqto-ui-container-compositor-and-agent-control.md). Proposed extensions cover [unified App Actions and host-owned action surfaces](0045-unified-app-actions-and-host-owned-action-surfaces.md) and [App egress, credential mediation, capability endpoints, and MCP adapters](0046-app-egress-credential-mediation-and-mcp-adapters.md).

This ADR refines [ADR-0027](0027-one-app-contract-oqtohost-surfaces-stay-bundled.md): the Host/Bridge/Gate capability principles and content-addressed Artifact direction survive, while App identity is sharpened into Definition, Installation, Instance, binding, and requester-local App Content. It generalizes [ADR-0036](0036-interactive-output-parts-declarative-first.md): declarative interaction and sandboxed web content are portable App presentations, whether embedded in Chat or opened as OqtoUI Container Content.

## Context

Agent-built Apps need to work in React/Web, future GPUI desktop, native iOS, and Pi without loading untrusted generated code into the host process. A declarative component vocabulary gives safe native presentation but can become an expressiveness ceiling; sandboxed web content gives an escape hatch but must not inherit Oqto credentials or unrestricted capabilities.

“Global App” also conflates five facts: what immutable App exists, where it is available, which durable object owns one use, what data that use may access, and where its requester-local Content presentation is placed. Filesystem placement under `.oqto/` is excellent for agent authoring and portability, but paths are mutable, copyable, mountable, symlinkable, and agent-writable.

## Decision

### Apps provide declarative-native and/or sandboxed-web presentations

An App Definition may provide:

- **declarative presentation:** validated UI data rendered by each host’s native registry; suited to forms, choices, tables, dashboards, approvals, and compact/standard use;
- **sandboxed web presentation:** a content-addressed MCP Apps resource rendered in an iframe/WebView/Glimpse host with CSP, navigation and capability allowlists, bounded resources, egress policy, and a typed Bridge; suited to canvases, simulations, advanced charts, image tools, and rich expanded/focused use. The bundle is served from a distinct opaque/non-Oqto origin or custom scheme without Oqto cookies; CSP defaults to no scripts/network beyond the packaged bundle, and any egress goes through a granted Host capability governed by [ADR-0030](0030-workspace-egress-policy-pluggable-enforcers.md) and [ADR-0035](0035-unified-egress-tiers-traceability-pluggable-enforcers.md). WebView/Glimpse hosts must additionally deny top-level navigation and direct network requests through their native navigation/resource delegates.

No agent-authored React, Rust, Swift, or other code loads into an OqtoUI host process. Native iOS signing constraints and multi-user trust make this a contract, not a preference. App Sidecars remain governed by ADR-0027 and require their separate server-side trust/placement decision; this ADR does not turn presentation bundles into Sidecars.

The declarative manifest names a versioned profile and required component capabilities. Hosts advertise supported profiles/components. The host selects a supported presentation when opening App Content, considering allocated fidelity and user preference; it fails explicitly or uses a declared fallback when requirements are unavailable.

An active presentation is not automatically replaced merely because a resize crosses a fidelity threshold. Hot switching is allowed only when the App declares compatible versioned instance-state serialization. Both presentations otherwise share host-managed App-instance storage and the same binding/capability grants, but may expose different presentation depth without claiming feature identity.

### App facts remain independent

- **App Definition:** immutable identity of a versioned, content-addressed bundle and manifest. Source is not a Definition until an explicit deterministic build/validation action produces the bundle.
- **App Installation:** makes a Definition available under a deployment, Account, Workspace, or work-directory owner and records provenance.
- **App Instance:** one durable/logical use of an installed Definition.
- **App binding:** explicit durable owner/data context for the Instance: deployment, Account, Workspace, work directory, Session, or a resource addressed beneath one of them.
- **App Content:** one requester-local OqtoUI Container Content presentation of the Instance under ADR-0041; its rendered presentation is a View.

“Global” is not a scope. Use the narrowest durable binding that contains all required data. A resource under a work directory is addressed by stable work-directory id plus policy-checked relative resource reference; host paths are never public resource identity.

A Definition declares supported bindings and may declare a default binding kind. Instance creation records exact public owner ids. Missing or ambiguous binding fails with supported choices. Binding is never inferred from title, selected route, basename, visible order, or dock location.

An Installation constrains availability:

- work-directory Installation: may bind only that work directory, its Sessions, and its resources;
- Workspace Installation: may bind that Workspace, its work directories, Sessions, and resources;
- Account Installation: may bind the Account or a Workspace/work directory/Session/resource that the Account is currently authorized to use, subject to a separate grant;
- deployment Installation: may be instantiated only where deployment policy and the acting Account/Operator Identity authorize it.

Promotion to a broader Installation is explicit. A technically valid but broader-than-required binding is rejected by policy or requires an explicit broader grant; the host never silently widens the default.

### App copy transfers availability, not Instance data

Copying or sharing an App creates availability for a target owner from the same immutable Definition; on the same deployment this may be another Installation reference rather than duplicated artifact bytes. Copying editable `.oqtoapp` source into a target work directory is a separate source-authoring action. Neither operation copies an existing Instance or its binding.

The default is categorically data-isolated: no authoritative bound files/resources, Account-private KV/preferences, presentation state, grants, secret bindings, credentials, or runtime state move with an App Definition, Installation promotion, source copy, or new target Installation. A target Instance starts fresh except for immutable packaged fixtures or declared initialization defaults. A Workspace target may later support a fresh Workspace-owned shared Instance or member-local Instances according to the App's supported ownership/binding model; neither form inherits source Instance data implicitly.

V0 supports code-and-availability copy only. A request to include Instance data fails explicitly as unsupported rather than guessing through a generic `include_data` switch. The canonical optional data-transfer interface is deferred until concrete Apps establish recurring semantics: versioned single documents (live notes/dgrmr), schema-aware directories and conflicts (file CRM), and connector references with mandatory target-side secret rebinding (YouTube). Likely mechanisms such as explicit resource selection, provenance, atomic/versioned copy, schema migration, and pinned export/import operations remain hypotheses until that evidence exists. Any future data copy is a separately authorized operation and creates an independent snapshot unless synchronization is explicitly designed; grants and secrets are never copied and must be granted or rebound at the target.

### Capability enforcement follows Instance and binding, not Content placement

Apps receive small orthogonal serializable Host capabilities such as files, Session input, instance storage, navigation, and dialogs. A manifest requests authority-bearing capabilities; an authorized Account or Operator Identity grants them; the App cannot self-grant. Theme observation and Presentation Context are baseline presentation environment rather than permissions: every active presentation receives bounded semantic Oqto theme tokens, color scheme, density, reduced-motion state, and actual Container bounds with change notifications. They expose no DOM, theme source files, arbitrary computed styles, or authority to change the user's theme. Changing, installing, or globally persisting appearance remains a separate privileged action.

Server-affecting capabilities—files, Session/agent action, egress, shared storage—flow through the runner-side Gate. The backend authenticates the acting Account/Operator Identity and membership/grant; the runner executes under the target work directory/Workspace Principal and verifies that mapped execution scope before exercising the capability. Neither Account authorization nor Principal isolation substitutes for the other. Host-local presentation operations such as theme reads, local navigation, or dialogs are allowlisted by the client Host and cannot confer server authority; any resulting server action still crosses the Gate. A Bridge only transports these contracts over `postMessage`, Glimpse JSON, `WKScriptMessageHandler`, or declarative callbacks.

Docking, resizing, or opening App Content somewhere else never changes Installation, binding, grants, or shared state. Workspace-bound shared instance state requires a versioned concurrency/CAS contract before collaborative mutation ships; local layout placement is not that contract.

### Recognized filesystem roots imply an Installation candidate, never authorization

Filesystem convention is agent-native:

- `<work-directory>/.oqto/apps/<app>/` proposes a work-directory Installation for the most-specific registered containing work-directory id;
- `$XDG_DATA_HOME/oqto/apps/<app>/` (or its platform-equivalent Account data directory) proposes an Account Installation;
- operator release-manifest App roots propose deployment Installations and are preserve-first operator config;
- Workspace Installation uses a canonical Workspace-owned catalog/root introduced with its catalog implementation, never a guessed common parent of work-directory paths. Until that root has its own reviewed contract, filesystem discovery creates no Workspace Installation.

If containing work-directory resolution is tied or outside every recognized root, discovery fails explicitly. The resolver canonicalizes paths, rejects symlink escape, validates bounded manifests/assets without executing code, and preserves unknown fields.

Discovery produces a source candidate. An agent or Account may invoke the runner-mediated build/validation action for a work-directory or Account candidate; successful validation produces an immutable Definition and activates/updates that Installation reference with audit provenance. This is safe without separate installation approval because activation grants no capability and executes no source: declarative data remains validated and web code remains sandboxed. Capability grants, Workspace/deployment promotion, and operator roots require their respective authorized Account/Operator decision. Editing source does not mutate a Definition or silently update running Instances.

Copying the same source/bundle into another work directory creates another Installation of the same Definition (or a new Definition after changed-source build) with a different owner. It transfers no instance state, grant, approval, or authority. Work-directory → Workspace/Account/deployment promotion creates a reviewed new Installation; moving files never promotes access.

Instances pin a Definition version. Updating an Installation makes a new version available but never mutates a running Instance in place. For the ordinary one-use-per-binding flow, publication creates or selects the Instance pinned to the new current Definition and marks prior Instances for the same Installation and binding as `superseded`. Superseded Instances remain auditable and may retain migration source state, but hold no executable grant, cannot open new Content, and do not appear in the normal launcher. Definition upgrade/migration is explicit and validated against the App's versioned state schema.

Multiple active Instances of one Installation are valid only when the App and user explicitly create distinct logical uses or bindings—for example separate project profiles, dashboards, Account-private versus Workspace-shared uses, or bindings to different resources. Republishing changed source alone is not user intent to create another logical App use and must not present old and new Definition-pinned Instances as peers.

Closing App Content is requester-local presentation disposal only: it does not revoke grants, delete the Instance, or uninstall the App. Every closable App presentation must expose a visible host-owned close control; hover-only controls and App-rendered close affordances are insufficient, and touch targets follow the Host's mobile accessibility rules.

Uninstall is a host-owned Installation lifecycle action, not closing Content, revoking one grant, or deleting source. After reauthorizing the acting Account against the Installation owner, uninstall atomically prevents new Instances, suspends all current Instances, revokes their grants, emits lifecycle events that terminate open Bridges and pending calls, removes the Installation from normal launch/discovery results, and preserves immutable Definitions and audit records. Bound files/resources, shared domain data, Account-private KV, and `.oqtoapp` source are preserved by default. Deleting private settings or source is a separate explicit choice with its own authorization and clear scope; uninstall never silently deletes authoritative data. Deleting a binding owner revokes grants and leaves an auditable unavailable/tombstoned Instance until explicit retention cleanup.

### Artifacts and extension vocabulary remain bitter-lesson-proof

Apps are ordinary inspectable workspace source, manifests, lockfiles, tests, and relative assets. Deterministic builds produce self-contained immutable bundles with no use-time dependency on the model or builder. Agents receive capability discovery, authoring skills, fail-loud validation, and preview/interaction loops across supported hosts and sizes.

Oqto owns mechanism: identity, persistence, validation, deterministic build, packaging, authorization, Gate enforcement, sandboxing, egress, bridges, and diagnostics. Humans/agents own judgment: what App to build, visual design, assets, presentation choices, and App behavior. Declarative vocabulary grows only by promoting recurring proven patterns; sandboxed web remains the expressive escape hatch for stronger models.

## Consequences

- One App can have a native compact presentation and rich web presentation without becoming two Installations or Instances.
- Portable Apps work in web, GPUI, iOS, and Pi/Glimpse hosts, but arbitrary generated native plugins remain forbidden.
- `.oqto/apps` is a discovery/build input, not an execution directory or permission boundary.
- App source changes and promotions are explicit lifecycle events with auditable provenance; ordinary republish supersedes the prior Instance rather than cluttering the launcher with Definition history.
- Close, revoke, uninstall, source deletion, and data deletion are distinct host-owned actions with different effects.
- Sharing, promotion, Definition reuse, and editable-source copy transfer no Instance data, preferences, grants, or secrets by default; optional data transfer remains fail-loud and deferred in v0.
- Existing `window.apphost` behavior remains migration input under ADR-0027; new work targets one Host contract rather than extending the old stringly API.

## Rejected alternatives

- **Declarative-only Apps:** safe and native but an expressiveness ceiling.
- **Web-only Apps:** expressive but weak compact/native presentation.
- **Unsandboxed generated native/React plugins:** incompatible with multi-user and mobile trust/signing.
- **One generic `global` scope:** conflates deployment, Account, availability, binding, and Content placement.
- **Filesystem path as authority:** mutable and agent-writable; suitable provenance/default, unsafe grant source.
- **Automatic rebuild/update on source change:** makes Definitions mutable and execution nondeterministic.
- **Resizing always swaps native/web presentation:** destroys in-progress state unless explicitly supported.
- **Treat every republished Definition as another peer App Instance:** rejected because immutable authorization history would leak into ordinary launcher UX and imply user intent that did not exist.
- **Treat close, revoke, source deletion, and uninstall as synonyms:** rejected because presentation placement, runtime authority, availability, editable source, and authoritative data have independent ownership and retention.

## Verification

Before agent-built Apps ship, tests and real traces must prove:

1. Definition builds are deterministic/content-addressed; source edits do not mutate an installed Definition or running Instance.
2. Declarative capability negotiation rejects unsupported profiles/components with available alternatives; declared fallback behavior is deterministic.
3. Web content is isolated from Oqto origin credentials; browser, WKWebView, and Glimpse tests prove default-denied navigation/direct network, while granted egress/tools/files traverse the Host and Gate.
4. Presentation selection and optional hot switching preserve the declared instance-state contract; ordinary resize never destroys active interaction state.
5. Work-directory Installations cannot bind broader/different owners; Account/deployment installs still require authorization and separate grants for each binding.
6. Wrong-owner, copied, nested, tied, outside-root, symlink-escaped, merely-redocked, and unapproved-promotion cases cannot broaden capability.
7. Server capabilities require both acting Account/Operator authorization and target Principal execution isolation at the runner Gate; host-local presentation capabilities cannot bypass either.
8. An agent discovers host capabilities, authors source, self-corrects precise validation errors, builds, previews target modes, and reproduces the same artifact.
9. Revocation, uninstall, explicit Definition upgrade/migration, and binding-owner deletion leave no executable stale grant and preserve the declared audit/data-retention outcome.
10. Copying to private work-directory, shared Workspace, Account, and deployment targets creates no source Instance data, KV/preferences, grants, secret bindings, credentials, or presentation state; v0 requests to include data fail explicitly, and target Instances start from fixtures/defaults only.
11. When a non-web host supports Apps, the same Definition/Instance/binding and Host contract pass that host’s native declarative and sandboxed-web conformance tests.
12. Republishing changed source for the same Installation and binding leaves exactly one current launchable Instance; prior Instances are superseded, ungranted, absent from normal launch results, and available only through authorized audit/history surfaces.
13. Explicitly created Instances with distinct logical uses/bindings remain independently launchable and isolated; publication alone cannot create this user-visible multiplicity.
14. Closing App Content has no Installation, Instance, grant, KV, source, or bound-resource side effect and is always available through visible accessible host chrome.
15. Uninstall tests prove new launches fail, all open Bridges suspend immediately, pending/future calls fail, grants are revoked, normal discovery omits the Installation, immutable audit records remain, and bound resources/KV/source survive unless separately selected and authorized for deletion.
16. Every active App receives semantic theme and Presentation Context without a permission request; tests prove these baseline reads expose no DOM/config source and cannot mutate global appearance, while theme changes propagate to the App.
