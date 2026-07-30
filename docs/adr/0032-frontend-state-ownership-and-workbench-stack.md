# Frontend state ownership and Workbench stack

## Status

Accepted (2026-07-30). Tracked by `oqto-e0n1`. Implements the shell decision in [ADR-0031](0031-session-centric-workbench-shell.md), preserves public Session identity from [ADR-0023](0023-one-public-oqto-session-id-harness-ids-adapter-local.md), and relies on the replay/drift contract in [ADR-0024](0024-transport-neutral-runner-wire-and-resumable-connections.md).

## Context

The current frontend duplicates state across routes, React contexts, component state, local storage, query caches, WebSocket managers, global browser events, and optimistic caches. Combined hooks expose unrelated UI and Session concerns, while large React hooks own transport, projection, persistence, and rendering behavior together. The result is unnecessary rerendering and timeline bugs that are difficult to test outside React.

The rebuilt Workbench must make navigation feel immediate and preserve every durable message through streaming, Session switching, reconnect, compaction, and reload. It must also support authenticated deep links and a real-time file manager without turning a state library into a new global authority.

## Decision

### 1. Every state category has one owner

| State category | Owner |
|---|---|
| shareable Workspace/work-directory/Session/file selection | TanStack Router |
| request/response server state and cache | TanStack Query |
| durable history plus live Session projection | framework-independent `SessionTimeline` module |
| local work-area tabs, pinning, active tab, panel sizes | narrowly scoped Zustand Workbench store |
| unsaved document content and recovery draft | `DocumentEditor` module |
| Chat composer draft | Session draft module |
| filesystem snapshot and live changes | runner watcher plus frontend file module |
| theme, language, account preferences | preferences module |
| validated form data | independent Zod schemas; React Hook Form is the React adapter |

No adapter or view may create a second authority for one of these categories. TanStack Query does not own the streaming timeline. Zustand does not hold server data, messages, file contents, API calls, or WebSocket connections. Routes identify shareable context; the complete personal tab collection remains local device state.

### 2. Routes use public identities and restore intent

TanStack Router owns typed authenticated routes for the shell, retained Surfaces, Workspace, work directory, Session, and optional active file context. It subsumes the current dynamic Surface registry as navigation authority; it does not turn the per-work-directory Workbench into a Surface.

The backend mints an opaque public `work_directory_id` when a work directory is registered in a Workspace. It remains stable across display-name, mount/path binding, and Placement changes; deleting that work-directory record and registering a new one mints a new ID. Host paths and typed mount sources are binding facts behind the ID, not route identity. Public routes and links use `workspace_id`, `work_directory_id`, and the Oqto Session ID from ADR-0023, never raw host paths or harness-native IDs.

Authentication returns the user to the requested route; authorization failures render an explicit access state. File paths inside the authorized work directory may be encoded as route payload after the IDs, but are resolved and policy-checked through the file interface. File line/range links are deferred, but the route shape must permit later extension.

### 3. Live Session behavior is a deep, framework-independent module

`SessionTimeline` presents one small interface to React and tests:

```ts
interface SessionTimeline {
  subscribe(listener: () => void): () => void;
  getSnapshot(): TimelineSnapshot;
  send(command: SessionCommand): Promise<void>;
}
```

Its implementation owns durable bootstrap, stable-identity reconciliation, delivery ordering, streaming overlay, reconnect/resume/drift recovery, compaction markers, background updates, and terminal state for one Oqto Session. React consumes snapshots through `useSyncExternalStore`. It does not mint public Session IDs or reconcile by text, index, array length, or visible order.

The interface is the shared test surface. Timeline correctness is proven with pure invariant/property tests, recorded trace replay, and browser reconnect/switch scenarios before views depend on it.

### 4. Files use a runner-mediated watch contract

A real file manager cannot be implemented by scattered refresh calls. The runner exposes an authorized work-directory watcher with:

- an initial directory snapshot/generation;
- ordered create, modify, move, and delete events;
- gap/overflow detection;
- an explicit recovery snapshot after drift;
- stable work-directory scoping.

The frontend file module joins snapshot and events, invalidates/query-updates file metadata in one place, refreshes read-only previews, and protects unsaved edits with expected-version saves. Conflicts never silently overwrite local work. Polling is not an accepted substitute.

### 5. Use a deliberately small client stack

- React, TypeScript, and Vite remain the SPA/PWA foundation.
- TanStack Router owns routes; TanStack Query owns request/response state; TanStack Virtual is allowed behind bounded list/tree modules after proving stable variable-height scrolling.
- Zustand is provisional and limited to the versioned local Workbench layout/tab schema.
- React Hook Form remains the form adapter; Zod schemas stay React-independent.
- CodeMirror 6 is the practical text/code editor. The existing MDXEditor may back rich Markdown only if a prototype proves performance, theme fit, and bundle cost; CodeMirror Markdown is the fallback.
- i18next ships English and German from the first usable Workbench and supports feature-local language expansion.
- Tailwind CSS 4 consumes semantic roles and variants from the shared design system. Per [ADR-0017](0017-design-system-vendored-via-byt-not-npm-publish.md), `@byteowlz/design-system` becomes the canonical home for base24 theming and reusable shadcn-style primitives rather than preserving Oqto's divergent local primitive set.

## Rejected alternatives

- **One global React context/store:** hides ownership, broadens rerenders, and recreates `useApp()` under another name.
- **TanStack Query for the live timeline:** query cache semantics do not provide the Session replay, overlay, and convergence contract.
- **React hooks as the timeline implementation:** couples correctness to rendering and makes the wrong surface the primary test seam.
- **Frontend polling for files:** cannot provide reliable external-change semantics or gap detection.
- **Redux or an unrestricted Zustand store:** unnecessary breadth for local layout state and an attractive place to duplicate server state.
- **TanStack Start/Next.js/full framework migration:** the browser/PWA product does not require SSR and gains no value proportional to migration cost.
- **Adopt every TanStack package for uniformity:** libraries are selected for an owned state category, not brand consistency.

## Consequences

- Existing transport and projection behavior must be extracted or replaced behind the new interfaces; legacy hooks are not imported into the Workbench.
- A runner filesystem watcher is a backend prerequisite for the new Files area.
- Work-directory registration and migration must add the backend-minted `work_directory_id`; existing path-addressed records need a one-time preserve-first backfill before deep links ship.
- Persisted Zustand and editor-draft schemas require versioning and migration tests.
- Performance and correctness are verified at realistic local scale: 250 work directories, 2,000 Sessions in one work directory, 20,000 timeline entries, 50,000 files, 10 concurrent Sessions, and 5 simultaneous streams.
- Technology choices remain replaceable at their seams; views must not depend on library-specific cache/store internals.
