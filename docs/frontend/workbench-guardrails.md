# Workbench architecture guardrails

These guardrails implement [ADR-0031](../adr/0031-session-centric-workbench-shell.md) and [ADR-0032](../adr/0032-frontend-state-ownership-and-workbench-stack.md). They combine mechanical build gates with named human-review gates; both are release requirements. The new shell does not accept functional code until the immediate mechanical checks below exist in CI/package gates.

## Authority and exceptions

Only Tommy may approve an exception. An agent may propose one but must stop rather than add a suppression, weaken a rule, expand a baseline, or introduce an undocumented compatibility path.

An approved exception records:

- rule and exact scope;
- reason the normal interface cannot work;
- evidence for the trade-off;
- owner and removal condition;
- linked `trx` issue.

No permanent `ignore`, broad lint disable, or “temporary” adapter is valid without that record.

## Scope and dependency direction

New shell code lives under `frontend/src/workbench/`. The temporary authenticated route is `/workbench`; fixtures live at admin/dev-only `/workbench-lab`.

Allowed dependency direction:

```text
routes -> surfaces -> modules -> adapters -> generated protocol/client types
                   \-> shared design-system primitives
```

- Routes compose; they do not fetch, subscribe, reconcile, or persist.
- Surfaces render one product concern and call module interfaces.
- Modules own behavior and expose one narrow interface shared by callers and tests.
- Adapters translate transport/storage/library behavior at real seams.
- Reverse imports and sideways feature imports are forbidden.
- Broad barrel exports that erase module ownership are forbidden.

### Forbidden legacy imports

Workbench code may not import:

- `@/hooks/use-app`;
- `@/components/contexts/*`;
- legacy `AppShellRoute`, `SessionScreen`, or `ChatView` implementations;
- legacy WebSocket managers/clients directly;
- deprecated root hook wrappers;
- current Surface/App registries as a navigation authority.

Canonical generated types, protocol types, authenticated clients, pure utilities, and proven renderers may be admitted only through an explicit adapter allowlist. Reuse means moving/extracting behavior behind a new interface, not layering a new view over legacy state.

## State rules

The ownership table in ADR-0032 is exclusive.

- No server data, messages, file contents, API calls, or sockets in Zustand.
- No streaming timeline in TanStack Query.
- No selected Workspace/work-directory/Session duplicated outside Router state.
- No transport object, cache, or persistence implementation exposed through React props/context.
- No `window` custom events, mutable module globals, or DOM-query coordination.
- No default context containing no-op functions; a missing provider fails loudly.
- No optimistic identity crosses a public API or durable store.
- Persisted local schemas are versioned and have migration tests.
- All subscriptions expose deterministic unsubscribe/teardown behavior.

## Interface and size budgets

These are mechanically measured default hard limits:

- React implementation file: 300 non-comment source lines;
- hook/module implementation file: 400 non-comment source lines;
- public module interface: at most 7 exported operations and no unstructured options bag;
- named options/config/input bags: at most 8 fields;
- React view props: at most 8 fields, including wrapped `memo`/`forwardRef` components.

Exceeding a limit requires Tommy's documented exception. Splitting files without reducing caller knowledge does not satisfy the rule. Prefer a deep module: small interface, substantial behavior, one test seam.

Human architecture review separately verifies one state authority and one reason to change per module. The reviewer records the ownership table and interface assessment in the linked `trx` issue; these semantic judgments are not claimed as AST-enforced checks.

## React and browser rules

- Raw `useEffect` is forbidden in Workbench code. Add an approved named lifecycle hook only when synchronization with an external system is unavoidable; encode the lifecycle invariant in its interface and tests.
- Direct `fetch`, `WebSocket`, `localStorage`, `sessionStorage`, timers, observers, and service-worker calls are adapter-only.
- Rendering is pure: no navigation, mutation, subscription, or persistence during render.
- Inactive Chat tabs do not subscribe React views to token-level updates.
- User-facing actions go through the typed command registry so buttons, menus, shortcuts, mobile sheets, and agent UI intents share one implementation.
- Agent UI intents are a small fixed shell-guidance protocol scoped to Account, Workspace, work directory, and Session. They may implement reversible actions such as semantic-target spotlight/focus only after a user asks for guidance. Arbitrary JavaScript, CSS selectors, or wholesale command-registry exposure are forbidden. This is distinct from ADR-0027's dynamic App action discovery: App capabilities remain search/describe/call through the Gate and are never flattened into UI-intent tools.

## Design-system and content rules

- Workbench imports reusable primitives from `@byteowlz/design-system`; it does not copy local shadcn components.
- No inline hex colors, ad hoc shadows, arbitrary radius systems, or feature-owned theme roles.
- New visual capabilities use semantic tokens/variants upstream in the design system.
- User-facing strings use i18next namespaces. English and German parity is checked.
- Status is never communicated by color alone.
- Hover may enhance but may not be the only discovery or interaction mechanism.
- Keyboard operation, visible focus, screen-reader names, reduced motion, and touch target sizes are acceptance requirements.

## Immediate mechanical gates

These checks must be implemented before functional Workbench code merges:

1. **Import architecture:** AST/import graph check for layer direction, cycles, and forbidden legacy paths.
2. **Forbidden browser/state APIs:** AST checks limiting effects, storage, sockets, fetch, timers, and globals to approved adapters.
3. **Size budgets:** source-line and public-interface/prop budget check with a Tommy-owned exception manifest.
4. **Design tokens and i18n:** reject hardcoded colors/effects and untranslated user-facing strings in Workbench paths.
5. **Dead-code hygiene:** TypeScript, Biome, Oxlint, dead-export, and unused-direct-dependency gates; warnings fail.
6. **Bundle budget:** record route/chunk sizes and fail unjustified regressions; Terminal, Browser, editors, Gallery, and Apps remain lazy chunks.
7. **Generated types:** protocol type generation/format checks remain clean.
8. **Invariant fixtures:** compile/runtime tests reject duplicate Router selection state, optimistic IDs at public/durable adapters, no-op context defaults, and ownership violations that cannot be determined from imports alone.

The check implementation and its fixtures are reviewed before feature implementation. Baselines start at zero; they are not copied from the legacy shell. Semantic rules that cannot be proven mechanically are listed as human architecture-review gates rather than misrepresented as lint checks.

## Required test layers

- **Vitest:** pure module invariants, property tests, persisted-state migrations, snapshot/tail joins, watcher gap recovery, document conflicts.
- **React Testing Library:** focused keyboard, accessibility, error, empty, loading, conflict, and command-availability interactions.
- **Playwright:** real browser desktop/mobile journeys, screenshots, reload/reconnect, multi-tab/session switching, live file changes, auth deep links, and access denial.

A UI task is not complete with unit tests alone. Browser evidence includes the route, viewport, steps, screenshots where visual state matters, console/network errors, and observed result.

## Performance and scale gates

Measured on representative normal laptop and phone profiles with already-loaded data:

- sidebar/work-area tab response visible within 50 ms;
- cached Session switch within 100 ms;
- uncached useful Session content within 500 ms when the network permits;
- no input-blocking task over 50 ms;
- no full-shell rerender during token streaming.

Required fixtures:

- 250 work directories;
- 2,000 Sessions in one work directory;
- 20,000 timeline entries in one Session;
- 50,000 files;
- 10 concurrent Sessions;
- 5 simultaneous streaming Sessions.

TanStack Virtual is hidden inside bounded list/tree modules. Chat virtualization ships only after variable-height content, streaming, image loading, prepend/history loading, and scroll restoration show no visible jumps.

## Correctness gates

### Session timeline

Release is blocked by any duplicate/lost durable message, ordering violation, cross-Session event leak, incorrect tool attachment, reconnect/replay corruption, optimistic identity leak, stuck terminal state, incoherent compaction marker, or inactive-tab update loss. Reconciliation by text, index, array length, or visible order is forbidden.

### Files and editors

The Files area reflects agent, terminal, UI, and external filesystem changes. Watcher gaps trigger explicit snapshot recovery. Read-only previews refresh automatically. Unsaved edits use expected-version saves; conflicts offer compare/reload/confirmed overwrite. Closing dirty tabs requires save/discard/cancel and preserves a local recovery draft. No silent overwrite is permitted.

### Deep links

Authenticated links restore Workspace, work directory, Session, and optional file context using `workspace_id`, backend-minted `work_directory_id`, and the Oqto Session ID. Existing path-addressed work directories receive a preserve-first one-time ID backfill before links ship. Raw host paths and harness IDs never identify routes. Authorized users reach the target; unauthorized users receive an explicit access state without leaked metadata.

## Human architecture-review gates

Before each module merges, its linked issue records:

- the one state category it owns and confirmation that no second authority was introduced;
- its public interface, invariants, error modes, and teardown behavior;
- why the module has one reason to change;
- adapters admitted at its seams and evidence that each seam has real variation;
- any non-mechanical accessibility, language, or interaction judgment.

Only Tommy may approve an exception; ordinary approval confirms the module satisfies the rule without exception.

## Workbench Lab approval gates

Before functional shell implementation, Tommy approves realistic desktop/mobile fixtures for:

- navigation and work-directory/Session scope;
- Chat/File/workbench tab behavior and overflow;
- file headers for empty, tree, grid, image, text/code, Markdown, PDF, audio, video, conflict, and mobile states;
- tool-call presentation variants replaying real traces;
- task progress/activity-rail variants;
- Herdr-style Working, Blocked, Done, Idle, and Unknown states;
- loading, reconnecting, empty, error, access-denied, and offline states.

Mockup approval chooses interaction contracts, not implementation shortcuts.

## Cutover checklist

The admin-only `/workbench` route replaces the old authenticated shell only when:

- Chat and Files satisfy all correctness and performance gates;
- required file preview/edit types work;
- search ranks work directories, Sessions, and relevant Session content through sidebar and command palette;
- mobile/PWA core journeys pass;
- English/German and accessibility checks pass;
- layout, tabs, drafts, pins, and deep links restore correctly;
- file watcher recovery and Session replay/drift are proven in real placement/browser tests;
- no forbidden legacy import exists;
- every migrated feature has one owner and one implementation;
- the deletion diff for the old shell, route switch, and superseded compatibility paths is ready.

After cutover, the temporary switch and old shell are deleted. Indefinite dual-shell compatibility is forbidden.
