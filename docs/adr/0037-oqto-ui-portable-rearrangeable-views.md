# OqtoUI: portable rearrangeable Views with progressive fidelity

## Status

Accepted (2026-08-09). Tracked by `oqto-171p`. Companion App decision: [ADR-0038](0038-agent-built-apps-installation-binding-and-filesystem-discovery.md). [ADR-0041](0041-oqto-ui-container-compositor-and-agent-control.md) supersedes this ADR's persisted split-tree layout and refines Views into Container Content hosted by grid-managed Containers; the remaining decisions here stay accepted.

This ADR supersedes the **Workbench name, fixed desktop composition, temporary route names, and layer-oriented source tree** in [ADR-0031](0031-session-centric-workbench-shell.md) and [ADR-0032](0032-frontend-state-ownership-and-workbench-stack.md). It preserves their Workspace → work directory → Session hierarchy, Chat/Files core-default status, exclusive state-ownership table, framework-independent Session timeline, public identity rules, file-correctness requirements, parallel migration, and delete-after-cutover policy.

## Context

“Workbench” introduced a product noun users do not need, and its fixed navigation/work-area/Files composition understated the desired flexibility. Sessions, Chat, Files, Editor, Image Editor, Terminal, and App Content must be rearrangeable without giving each arrangement its own state implementation. A narrow Files View must stay fast and useful; the same Content, when allocated more fidelity, must grow into a modern high-performance file manager.

Oqto must also admit future GPUI desktop and native iOS clients without making React the product contract or forcing every host into a lowest-common-denominator widget toolkit.

## Decision

### OqtoUI replaces the Workbench concept and continues beside the old interface

**OqtoUI** is the platform-neutral user-interface contract for Oqto, not the name of one React implementation. During the pilot, `/oqto-ui` is authenticated and deployment-config gated (admin-only by default); deterministic scenarios exist only in development builds at authenticated `/dev/oqto-ui`. Scripted transport is not compiled into production. “Lab” is not product terminology.

The old interface stays operational until OqtoUI surpasses it in correctness, capability, measured performance, accessibility, English/German parity, and desktop/mobile/PWA behavior. Parallel development is migration, not permanent compatibility. At cutover OqtoUI becomes the authenticated root, the old interface and pilot switch are deleted, and `/oqto-ui` is removed or redirects temporarily under an explicit migration issue. `/dev/oqto-ui` remains development-build-only.

Live and scenario routes use the same catalog, timeline, Files, layout, and React implementations. Scenarios replace only external HTTP/event transport with scripted responses and recorded traces; there is no fixture runtime or second state model.

### OqtoUI hosts rearrangeable first-party Content

A **View** is the rendered presentation of one Container Content item hosted by OqtoUI. Initial Content kinds include Sessions, Chat, Files, Editor, Image Editor, and Terminal; Apps may provide App Content under ADR-0038 and ADR-0041. Chat and Files remain core defaults, but no Content kind is hardwired to a permanent left/center/right region.

Desktop composition follows ADR-0041's versioned Container compositor: a semantic Grid arranges Containers, and each Container holds an ordered stack of Container Content references. Users may dock, tab, resize, swap, close, pin, or focus Content. Moving Content changes presentation only; it never changes owner, data binding, capabilities, or authorization.

Every Container Content item has stable identity and an explicit owner: deployment, Account, Workspace, work directory, Session, App Instance, or an addressed resource beneath one of those owners. Deleted or inaccessible owners render an explicit unresolved View while preserving the Content reference.

Layouts are Account-namespaced, device/Screen-Mode-local state initially: they survive reload on that device but not storage removal or movement to another device. The client `layout` feature validates and migrates the versioned schema. Screen Modes such as `desktop`, `desktop-big-picture`, `tablet`, and `mobile` (renamed from "Layout Profiles" by ADR-0043) do not overwrite one another; cross-device synchronization is deferred. Unknown future Content kinds survive schema round trips and render as unavailable rather than being deleted. Layout documents contain references and sizes, never messages, file contents, sockets, or server authority.

Durable/server state follows the ADR-0032 owner; presentation state such as scroll, selection, focus, and pane mode is keyed by stable Content id. Multiple Chat Content items for one Session share its Session-owned composer draft but retain independent presentation state. Closing Content discards unpinned ephemeral presentation state without stopping/deleting its owner; a persisted/pinned restoration descriptor restores versioned presentation state when supported.

Mobile projects available Content into one focused destination, drawers, and switchers instead of applying the desktop Grid directly. First-party Terminal Content is work-directory-owned; opening and sending input requires current Account authorization and runner-mediated terminal commands. Placement never grants terminal access.

### Fidelity derives from the allocated container

First-party Views support semantic presentation modes **compact**, **standard**, **expanded**, and **focused**. OqtoUI derives the mode from each allocated container and platform constraints, using container queries for visual adaptation where possible. One View model backs all modes; fidelity is progressive disclosure, not feature forks.

Files progresses from a virtualizable tree, through toolbar/breadcrumb/search and lightweight preview, to multiple panes, separate rich preview/details panes, bulk operations, drag/drop, gallery modes, and large-directory interaction. Chat can add search, table of contents, branch and tool/context inspection. Editor progresses from preview to editing, split/diff, and conflict handling. Heavy renderers and workers load only when invoked; inactive tabs preserve owned state while suspending avoidable rendering and subscriptions.

Token streaming in one Chat must not rerender the shell, Sessions, Files, or another Chat. File events must not rerender Chat. Resizing must not refetch data.

### The React client is feature-oriented

The web implementation lives under `frontend/src/oqto-ui/`:

```text
app/          composition and routes
layout/       compositor, Containers, Content registry, layout schema/storage
sessions/     catalog and Sessions View
chat/         SessionTimeline and Chat View
files/        file projection and Files View
editor/
image-editor/
terminal/
platform/     canonical HTTP/WebSocket/storage adapters
dev/          scripted scenarios for /dev/oqto-ui
```

`app` composes features. A feature may depend on platform contracts and shared design-system primitives but not another feature’s internal state. `platform` depends on no product feature. Correctness-heavy models remain framework-independent and expose narrow external-store/command interfaces.

Before functional OqtoUI code lands, a second zero-baseline OqtoUI checker and guardrail document are created for this dependency graph. The existing Workbench checker continues unchanged over `frontend/src/workbench/` only; it is deleted with that prototype after equivalent OqtoUI gates exist. Neither checker is broadened or weakened to cover both architectures.

### Protocol and behavior are portable; rendering stays native

The canonical protocol, public identities, message parts, events, commands, layout schema, Container Content descriptors, Base24 role data, and conformance traces are platform-neutral contracts. Rust remains the wire-type source of truth; a client receives generated types for its language when that client is implemented. Stringly handwritten event mirrors are forbidden.

React/Web, GPUI/Rust, and SwiftUI/UIKit render Views with native technology. Oqto does not build a universal component abstraction across them. Sanitized canonical event/command traces and expected settled snapshots live under `contracts/oqto-ui/traces/`; runner/backend integration tests produce and validate candidate traces, and reviewed fixtures are committed. Session timeline, file projection, and layout-state transitions use that corpus to prove semantic equivalence; pixel layout and native interaction are tested per client.

A portable Rust client-core may be extracted after a second native client demonstrates enough shared behavior. Oqto does not impose WASM or UniFFI on the web client speculatively.

Host-neutral agent intents address semantic Container/Content kinds, owners, resources, and commands—never React components, CSS selectors, GPUI widgets, or Swift types. Optional compositor control follows ADR-0041's requester-local, revisioned, policy-limited transaction contract. Unsupported capabilities fail explicitly and report available alternatives.

### Big Picture is a Screen Mode, not another runtime

A Steam/Jellyfin-style Big Picture experience is an OqtoUI Screen Mode — a layout/input class — over the same catalog, Content, identities, and permissions. Semantic actions (`navigate`, `activate`, `back`, `search`, `menu`) map to keyboard, touch, gamepad, or remote input per client.

A work directory may optionally provide validated, relative, non-executable presentation data under `.oqto/` such as display name, icon, banner, poster, and tags. It never grants capabilities or changes ownership; missing values have deterministic fallbacks and unknown fields are preserved.

## Consequences

- ADR-0031’s visual composition becomes the initial desktop preset, not architecture.
- The first live vertical slice keeps one model path: scenario traces and real transport drive the same behavior.
- Work-directory ids, canonical generated client types, reliable timeline recovery, runner-mediated file-watcher recovery, expected-version saves, and client-owned versioned layout migration remain cutover requirements.
- Native clients can arrive incrementally without changing backend identity or App artifacts; rendering is intentionally platform-specific.
- Oqto supplies deterministic mechanism—identity, persistence, projection, layout transforms, validation, authorization—and leaves layout and presentation judgment as human/agent-authored data.

## Rejected alternatives

- **Keep “Workbench”:** unnecessary vocabulary and too suggestive of one fixed area.
- **Separate fixture and live runtimes:** duplicates behavior; only external transport may vary.
- **Fixed Sessions/Chat/Files columns:** useful preset, inadequate architecture.
- **One cross-platform component toolkit:** sacrifices native interaction without safely enabling generated native code.
- **Viewport-only responsive design:** wrong when users resize and rearrange individual Views.

## Verification

OqtoUI replaces the old interface only when evidence proves:

1. `/oqto-ui` and `/dev/oqto-ui` drive the same feature models and recorded live traces settle to identical snapshots.
2. Rearranging, resizing, tabbing, and focusing Content never changes ownership or authority and causes no unrelated Content rerenders/refetches.
3. Files and Chat demonstrate compact through focused fidelity without forked state and meet ADR-0032 scale/latency/correctness gates.
4. Session reconnect/reload converges to oqto-log with no duplicate, lost, reordered, or cross-Session content.
5. Desktop, Big Picture, tablet, and mobile Screen Modes preserve unknown Content and never overwrite one another; cache-clear/new-device behavior matches the explicitly device-local contract.
6. Required accessibility, keyboard/touch operation, English/German parity, PWA reconnect behavior, pilot-route authorization/build exclusion, and old-versus-new capability journeys pass in real clients.
7. When a GPUI or iOS client ships, it must pass the shared protocol/projection trace corpus plus its own native interaction, performance, and accessibility gates; a stub renderer is not cross-platform proof.
