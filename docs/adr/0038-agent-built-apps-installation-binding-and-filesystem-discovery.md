# Agent-built Apps: portable presentations, explicit binding, filesystem discovery without path authority

## Status

Accepted (2026-08-09). Tracked by `oqto-171p`. Companion UI-host decision: [ADR-0037](0037-oqto-ui-portable-rearrangeable-views.md).

This ADR refines [ADR-0027](0027-one-app-contract-oqtohost-surfaces-stay-bundled.md): the Host/Bridge/Gate capability principles and content-addressed Artifact direction survive, while App identity is sharpened into Definition, Installation, Instance, binding, and View. It generalizes [ADR-0036](0036-interactive-output-parts-declarative-first.md): declarative interaction and sandboxed web content are portable App presentations, whether embedded in Chat or opened as an OqtoUI View.

## Context

Agent-built Apps need to work in React/Web, future GPUI desktop, native iOS, and Pi without loading untrusted generated code into the host process. A declarative component vocabulary gives safe native presentation but can become an expressiveness ceiling; sandboxed web content gives an escape hatch but must not inherit Oqto credentials or unrestricted capabilities.

“Global App” also conflates five facts: what immutable App exists, where it is available, which durable object owns one use, what data that use may access, and where its View is docked. Filesystem placement under `.oqto/` is excellent for agent authoring and portability, but paths are mutable, copyable, mountable, symlinkable, and agent-writable.

## Decision

### Apps provide declarative-native and/or sandboxed-web presentations

An App Definition may provide:

- **declarative presentation:** validated UI data rendered by each host’s native registry; suited to forms, choices, tables, dashboards, approvals, and compact/standard use;
- **sandboxed web presentation:** a content-addressed MCP Apps resource rendered in an iframe/WebView/Glimpse host with CSP, navigation and capability allowlists, bounded resources, egress policy, and a typed Bridge; suited to canvases, simulations, advanced charts, image tools, and rich expanded/focused use. The bundle is served from a distinct opaque/non-Oqto origin or custom scheme without Oqto cookies; CSP defaults to no scripts/network beyond the packaged bundle, and any egress goes through a granted Host capability governed by [ADR-0030](0030-workspace-egress-policy-pluggable-enforcers.md) and [ADR-0035](0035-unified-egress-tiers-traceability-pluggable-enforcers.md). WebView/Glimpse hosts must additionally deny top-level navigation and direct network requests through their native navigation/resource delegates.

No agent-authored React, Rust, Swift, or other code loads into an OqtoUI host process. Native iOS signing constraints and multi-user trust make this a contract, not a preference. App Sidecars remain governed by ADR-0027 and require their separate server-side trust/placement decision; this ADR does not turn presentation bundles into Sidecars.

The declarative manifest names a versioned profile and required component capabilities. Hosts advertise supported profiles/components. The host selects a supported presentation when opening a View, considering allocated fidelity and user preference; it fails explicitly or uses a declared fallback when requirements are unavailable.

An active presentation is not automatically replaced merely because a resize crosses a fidelity threshold. Hot switching is allowed only when the App declares compatible versioned instance-state serialization. Both presentations otherwise share host-managed App-instance storage and the same binding/capability grants, but may expose different presentation depth without claiming feature identity.

### App facts remain independent

- **App Definition:** immutable identity of a versioned, content-addressed bundle and manifest. Source is not a Definition until an explicit deterministic build/validation action produces the bundle.
- **App Installation:** makes a Definition available under a deployment, Account, Workspace, or work-directory owner and records provenance.
- **App Instance:** one durable/logical use of an installed Definition.
- **App binding:** explicit durable owner/data context for the Instance: deployment, Account, Workspace, work directory, Session, or a resource addressed beneath one of them.
- **App View:** one local OqtoUI presentation of the Instance under ADR-0037.

“Global” is not a scope. Use the narrowest durable binding that contains all required data. A resource under a work directory is addressed by stable work-directory id plus policy-checked relative resource reference; host paths are never public resource identity.

A Definition declares supported bindings and may declare a default binding kind. Instance creation records exact public owner ids. Missing or ambiguous binding fails with supported choices. Binding is never inferred from title, selected route, basename, visible order, or dock location.

An Installation constrains availability:

- work-directory Installation: may bind only that work directory, its Sessions, and its resources;
- Workspace Installation: may bind that Workspace, its work directories, Sessions, and resources;
- Account Installation: may bind the Account or a Workspace/work directory/Session/resource that the Account is currently authorized to use, subject to a separate grant;
- deployment Installation: may be instantiated only where deployment policy and the acting Account/Operator Identity authorize it.

Promotion to a broader Installation is explicit. A technically valid but broader-than-required binding is rejected by policy or requires an explicit broader grant; the host never silently widens the default.

### Capability enforcement follows Instance and binding, not View placement

Apps receive small orthogonal serializable Host capabilities such as files, Session input, instance storage, navigation, theme, and dialogs. A manifest requests capabilities; an authorized Account or Operator Identity grants them; the App cannot self-grant.

Server-affecting capabilities—files, Session/agent action, egress, shared storage—flow through the runner-side Gate. The backend authenticates the acting Account/Operator Identity and membership/grant; the runner executes under the target work directory/Workspace Principal and verifies that mapped execution scope before exercising the capability. Neither Account authorization nor Principal isolation substitutes for the other. Host-local presentation operations such as theme reads, local navigation, or dialogs are allowlisted by the client Host and cannot confer server authority; any resulting server action still crosses the Gate. A Bridge only transports these contracts over `postMessage`, Glimpse JSON, `WKScriptMessageHandler`, or declarative callbacks.

Docking, resizing, or opening an App View somewhere else never changes Installation, binding, grants, or shared state. Workspace-bound shared instance state requires a versioned concurrency/CAS contract before collaborative mutation ships; local layout placement is not that contract.

### Recognized filesystem roots imply an Installation candidate, never authorization

Filesystem convention is agent-native:

- `<work-directory>/.oqto/apps/<app>/` proposes a work-directory Installation for the most-specific registered containing work-directory id;
- `$XDG_DATA_HOME/oqto/apps/<app>/` (or its platform-equivalent Account data directory) proposes an Account Installation;
- operator release-manifest App roots propose deployment Installations and are preserve-first operator config;
- Workspace Installation uses a canonical Workspace-owned catalog/root introduced with its catalog implementation, never a guessed common parent of work-directory paths. Until that root has its own reviewed contract, filesystem discovery creates no Workspace Installation.

If containing work-directory resolution is tied or outside every recognized root, discovery fails explicitly. The resolver canonicalizes paths, rejects symlink escape, validates bounded manifests/assets without executing code, and preserves unknown fields.

Discovery produces a source candidate. An agent or Account may invoke the runner-mediated build/validation action for a work-directory or Account candidate; successful validation produces an immutable Definition and activates/updates that Installation reference with audit provenance. This is safe without separate installation approval because activation grants no capability and executes no source: declarative data remains validated and web code remains sandboxed. Capability grants, Workspace/deployment promotion, and operator roots require their respective authorized Account/Operator decision. Editing source does not mutate a Definition or silently update running Instances.

Copying the same source/bundle into another work directory creates another Installation of the same Definition (or a new Definition after changed-source build) with a different owner. It transfers no instance state, grant, approval, or authority. Work-directory → Workspace/Account/deployment promotion creates a reviewed new Installation; moving files never promotes access.

Instances pin a Definition version. Updating an Installation makes a new version available but never mutates running Instances; migration is explicit and validated against the App's versioned state schema. Revoking a grant takes effect immediately at the Gate. Uninstall prevents new Instances and suspends existing ones without silently deleting their data. Deleting a binding owner revokes grants and leaves an auditable unavailable/tombstoned Instance until explicit retention cleanup.

### Artifacts and extension vocabulary remain bitter-lesson-proof

Apps are ordinary inspectable workspace source, manifests, lockfiles, tests, and relative assets. Deterministic builds produce self-contained immutable bundles with no use-time dependency on the model or builder. Agents receive capability discovery, authoring skills, fail-loud validation, and preview/interaction loops across supported hosts and sizes.

Oqto owns mechanism: identity, persistence, validation, deterministic build, packaging, authorization, Gate enforcement, sandboxing, egress, bridges, and diagnostics. Humans/agents own judgment: what App to build, visual design, assets, presentation choices, and App behavior. Declarative vocabulary grows only by promoting recurring proven patterns; sandboxed web remains the expressive escape hatch for stronger models.

## Consequences

- One App can have a native compact presentation and rich web presentation without becoming two Installations or Instances.
- Portable Apps work in web, GPUI, iOS, and Pi/Glimpse hosts, but arbitrary generated native plugins remain forbidden.
- `.oqto/apps` is a discovery/build input, not an execution directory or permission boundary.
- App source changes and promotions are explicit lifecycle events with auditable provenance.
- Existing `window.apphost` behavior remains migration input under ADR-0027; new work targets one Host contract rather than extending the old stringly API.

## Rejected alternatives

- **Declarative-only Apps:** safe and native but an expressiveness ceiling.
- **Web-only Apps:** expressive but weak compact/native presentation.
- **Unsandboxed generated native/React plugins:** incompatible with multi-user and mobile trust/signing.
- **One generic `global` scope:** conflates deployment, Account, availability, binding, and View placement.
- **Filesystem path as authority:** mutable and agent-writable; suitable provenance/default, unsafe grant source.
- **Automatic rebuild/update on source change:** makes Definitions mutable and execution nondeterministic.
- **Resizing always swaps native/web presentation:** destroys in-progress state unless explicitly supported.

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
10. When a non-web host supports Apps, the same Definition/Instance/binding and Host contract pass that host’s native declarative and sandboxed-web conformance tests.
