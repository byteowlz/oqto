# Session-centric workbench shell

## Status

Accepted (2026-07-30). Tracked by `oqto-e0n1`. Builds on the Workspace, work directory, Session, Surface, and App language in `CONTEXT.md`. The App integration follows the proposed direction in [ADR-0027](0027-one-app-contract-oqtohost-surfaces-stay-bundled.md) but remains deferred until that ADR is accepted.

## Context

Oqto's frontend grew by promoting capabilities into peer views. Chat, files, tasks, terminal, browser, canvas, memories, settings, Apps, slides, and administration accumulated separate navigation and state. The resulting shell obscures Oqto's product model: people delegate durable work to autonomous agents operating in real shared filesystems. The existing shell also couples navigation, selection, transport, and feature state so strongly that incremental decomposition has repeatedly moved complexity without removing it.

The product decisions are now explicit:

- a Workspace is the collaborative container;
- a work directory carries one agent persona through its instructions, memory, and filesystem context;
- a Session is one durable conversation with that agent;
- Chat and Files are the two core user surfaces;
- Files are permanently available because the shared filesystem is the common human/agent frame of reference;
- other capabilities must not earn permanent navigation merely by existing.

## Decision

### 1. Build a session-centric shell that hosts one selected Workbench

A **Workbench** is scoped to one work directory, as defined in `CONTEXT.md`. The **shell** is the authenticated application around it: Workspace/work-directory/Session navigation, account access, and routing to the separately retained Admin Surface. The authenticated shell is rebuilt around:

```text
Workspace -> work directory -> Session
```

The normal desktop composition is:

```text
workspace/work-directory/session navigation | tabbed work area | persistent Files
```

The work area always supports Chat tabs. Files provides a live tree and lightweight preview; a file can be opened as a persistent work-area tab for larger viewing or editing. Panels are resizable and support reversible focus/swap actions. Mobile/PWA preserves Chat and Files as peer destinations while optional tools move behind a workbench menu rather than permanent tabs.

### 2. Work-directory and Session scopes prioritize; they do not destroy state

A work-directory row has a separate disclosure control for its Sessions. Activating the row opens the complete work-directory Workbench; activating a Session focuses that Session's Chat and Session-owned tabs. Work-directory-owned tabs remain accessible. Changing scope reorders or filters presentation without closing tabs, stopping Sessions, or discarding drafts.

Tab ownership is explicit:

| Tab kind | Owner |
|---|---|
| Chat | Session |
| Browser | Session |
| file preview/editor | work directory |
| Gallery | work directory |
| Terminal | work directory |
| Memories | work directory |
| App | declared by the App instance/binding contract |

Closing a Chat tab closes only the view. It never stops or deletes the Session. Work-directory tabs persist across Session switches and page reloads on the current device; cross-device tab-layout synchronization is deferred.

### 3. Chat and Files are fixed; tools are optional and pinnable

Chat and Files are the only fixed product areas inside a Workbench. Terminal, Browser, Gallery, Memories, image editing, and future tools open on demand and may be pinned by the user. Agent-authored Apps are a core capability but open as work-area tabs through ADR-0027; they are not shell navigation. The first file-backed App tabs are work-directory-owned through their Data binding; any future Session-owned App must declare and enforce that different binding explicitly.

This supersedes ADR-0027's proposed bundled-Surface/navigation-list direction without changing the Surface/App trust distinction: Surfaces remain trusted shell code, but `lib/app-registry.ts` and the dynamic `appRegistry` mechanism are deleted after cutover rather than retained in a secondary role. Typed routes are the authenticated navigation authority. The Sessions Surface is subsumed by the default authenticated shell, which hosts the selected work-directory Workbench; the Workbench is not itself a Surface. Admin remains a separate, role-gated Surface and is rebuilt after the core Workbench around accounts/invitations, model lifecycle, propagation, and usage audit. The previously named Dashboard is not present in the current registry and is not reintroduced.

The standalone Agents Surface is removed because the work directory carries the agent persona. The current sldr Surface is removed now; presentation authoring may return only as a mature App after the proposed Host/Gate contract in ADR-0027 is accepted and implemented. The A2UI runtime is removed without compatibility; a future lean interactive-output mechanism requires a new decision. The standalone Canvas view, godmode/unlock progression, and full user Settings Surface are removed. Useful Canvas behavior moves into image editing. Account preferences become a small dialog; work-directory configuration remains contextual. Raw platform diagnostics remain CLI/logging concerns rather than Admin UI panels.

### 4. Rebuild alongside the old shell, then delete the old shell

A new shell hosting the selected Workbench is built behind admin-only `/workbench` and an admin-only “Try new workbench” entry. An admin/dev-only `/workbench-lab` uses realistic fixtures for layout, state, accessibility, performance, and visual testing. The rebuild may reuse canonical protocol, authentication, history, file, and design-system interfaces, but may not import legacy shell contexts or preserve their state model.

The parallel route is temporary. Cutover requires the verification contract in `docs/frontend/workbench-guardrails.md`. Once it passes, the new shell replaces the current authenticated shell and the old shell, route switch, and superseded compatibility paths are deleted in the same migration program.

## Rejected alternatives

- **Keep decomposing the existing shell:** previous decompositions reduced file-local size while retaining competing state authorities and feature-first navigation.
- **Full frontend rewrite:** authentication, protocol, durable history, file access, onboarding, and proven renderers are reusable; duplicating their behavior would create unnecessary risk.
- **Files as contextual inspection:** contradicts Oqto's shared-filesystem product premise.
- **Every capability as a permanent tab/view:** recreates the current navigation and state bloat.
- **One fixed role for the human:** Oqto permits directing, collaborating, and supervising; the default entry optimizes delegation without removing the others.

## Consequences

- Workbench tab ownership and scope become domain-visible concepts rather than incidental component state.
- New capabilities must fit Chat, Files, an optional tool, an App, Admin, or remain outside the shell.
- File viewing/editing and reliable live refresh are cutover requirements, not secondary enhancements.
- Browser state remains Session-scoped; file/editor/Terminal state remains work-directory-scoped.
- Advanced HTML Apps, multiplayer editing, BYOK/OAuth subscriptions, shortcut remapping, voice redesign, and advanced PDF/image tooling may follow without blocking the core cutover.
