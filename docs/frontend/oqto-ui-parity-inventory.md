# OqtoUI parity inventory (legacy UI → OqtoUI)

Status: working document for `oqto-a9j4.6`, first pass 2026-10-19. Feeds the cutover
decision in [ADR-0067](../adr/0067-oqto-ui-replaces-the-legacy-ui.md).

**Method of this pass:** static source scan only. Legacy surfaces were enumerated from
`frontend/messages/en.json` namespaces, `frontend/apps/*` and `frontend/features/*`; OqtoUI
coverage from `frontend/src/oqto-ui/**` (tests excluded). Nothing here is runtime-verified yet.
A row turns green only with a screenshot pair at the three Studio views (desktop, tablet,
phone) plus a behaviour check, recorded in the Evidence column.

Status values: **better** (OqtoUI clearly surpasses legacy), **same**, **partial**,
**missing**, **dead** (UI present but not wired), **drop** (deliberately not ported; needs a
reason), **?** (not yet determined).

## Findings that block any cutover

1. **Dead sidebar controls.** `src/oqto-ui/sessions/NavigationRail.tsx` renders Settings,
   Admin, theme toggle and Logout buttons without click handlers (only the language toggle is
   wired). Users would find no way to sign out or reach admin.
2. **No signed-in identity.** The rail shows the static string `oqtoUi.person.name` ("You");
   OqtoUI never reads the current user.
3. **No admin surface at all.** Legacy `apps/admin` (users, invite codes, sessions, metrics,
   models) has no OqtoUI counterpart; OqtoUI only reads `/api/admin/stats` for the status strip.
4. **No tool permission or agent-question dialogs.** Legacy
   `components/chat/permission-dialog.tsx` / `user-question-dialog.tsx` have no OqtoUI
   equivalent (OqtoUI has Selection, which is a different contract per ADR-0053).
5. **No account settings.** Password change, API keys, OAuth/provider connections and the
   Omni/QR device link exist only in legacy settings.

## Inventory

### Shell and navigation

| Surface | Legacy | OqtoUI | Status | Evidence |
|---|---|---|---|---|
| Session list / sidebar | `apps/sessions` | `sessions/NavigationRail.tsx` | partial | |
| Projects (create, rename, delete, from template) | `projects.*` | rail has "new project"; template flow not found | partial | |
| Command palette | `command.*` | `compositor/react/CommandPalette.tsx` | ? | |
| Layout: split, tabs, resize, keyboard | fixed panes | compositor (ADR-0041/0042) | better | |
| Mobile navigation | legacy mobile layout | `MobileCompositor.tsx`, `layout/MobileTopBar.tsx` | ? | |
| Status bar | yes | `sessions/SessionStatusBar.tsx` | ? | |
| Language EN/DE | yes | rail toggle | same | |
| Theme light/dark/system | yes | theme picker; rail toggle dead | partial | |
| Sign out | yes | dead button | dead | |
| Dashboard (stats, scheduler, working agents, trx, feeds, cards) | `dashboard.*` | not found | missing | |

### Chat

| Surface | Legacy | OqtoUI | Status | Evidence |
|---|---|---|---|---|
| Timeline rendering (markdown, tools, thinking, parts) | yes | shared canonical renderers | same | `vrun-v0qw` |
| Detail levels for tool calls/thinking | `pi.chatVerbosity` | `chat/ChatDetail.tsx` | ? | |
| Step collapse across messages | no | no (Desktop has it) | missing | |
| History paging, load-error + retry | added 0.5.10 | engine pages | ? | |
| Model picker | yes | `SessionMeta` statusbar | partial | |
| Reasoning level | `pi.reasoningLevel` | not found | missing | |
| Context gauge | yes | `SessionMeta`, `ChatPane` | ? | |
| Abort / stop agent | yes | ? | ? | |
| Queue / steer while busy | `models.queueText` | ? | ? | |
| Tool permission dialog | `permission-dialog.tsx` | not found | missing | |
| Agent question dialog | `user-question-dialog.tsx` | Selection (different contract) | ? | |
| Search messages | `sessions.searchMessages`, `search.*` | not found | missing | |
| Rename / delete / bulk delete sessions | yes | rename/delete present in places | ? | |
| Fork / branch graph | `command.openBranchGraph` | fork action in corner chrome; no graph | partial | |
| File upload / attachments | `chat.uploadFile` | not found | missing | |
| Dictation / voice conversation | yes | dictation in `ChatPane` | partial | |
| Copy message / copy all / quote | yes | ? | ? | |
| Default assistant chat setup | `assistant.*` | not found | missing | |

### Workspace panes

| Surface | Legacy | OqtoUI | Status | Evidence |
|---|---|---|---|---|
| Files browse/preview/ops | `FileTreeView`, `PreviewView` | `files/*` (Miller columns, keyboard-first) | better? | `oqto-a9j4.29` |
| File upload / new / rename / delete | yes | ops present; upload not found | partial | |
| Markdown WYSIWYG editor | `MarkdownWysiwygEditor` | not found | missing | |
| Lightbox / video preview | yes | not found | missing | |
| Terminal | `TerminalView` | `terminal/TerminalPane.tsx` | ? | `oqto-a9j4.33` |
| Browser view | `BrowserView` | not found as a pane | missing | |
| Canvas | `CanvasView` | not found | missing | |
| Git | none | `git/*` | better | |
| Issues (trx) | `TrxView` | `issues/IssuesPane.tsx` | ? | |
| Todos / task progress | yes | `TodosPane`, `TaskProgress` | ? | |
| Memories (mmry) | `MemoriesView`, settings mmry tab | not found | missing | |
| Oqto apps (catalog, permissions, runtime frame) | `AppView`, `RuntimeOqtoAppFrame` | not found | missing | |
| Gallery | no | `gallery/GalleryPane.tsx` | better | |
| sldr | `apps/sldr` | fixture only | missing | |
| Agents catalog (install/configure) | `apps/agents` | not found | missing | |
| Workspace settings (default model, sandbox profile, skills, extensions) | `WorkspaceOverviewForm` | not found | missing | |
| Runner targets / provider login | partial | `RunnerTargets.tsx` | better? | |

### Settings and account

| Surface | Legacy | OqtoUI | Status | Evidence |
|---|---|---|---|---|
| Appearance / theme customizer | yes | `ThemeCustomizer`, `ConfigLens` | better? | |
| Keyboard shortcuts | help list | configurable bindings | better | |
| Help / tips | yes | not found | missing | |
| Change password | yes | not found | missing | |
| API keys | `ApiKeysPanel` | not found | missing | |
| OAuth / connected providers | yes | not found | missing | |
| Device link / QR (Omni) | yes | not found | missing | |
| Passkeys, MFA, sessions/devices | no (ADR-0061) | no | new | |

### Admin

| Surface | Legacy | OqtoUI | Status | Evidence |
|---|---|---|---|---|
| Users | `UsersPanel` | none | missing | |
| Invite codes | `InviteCodesPanel` | none | missing | |
| Sessions (kill, force stop) | `SessionsPanel` | none | missing | |
| Metrics (CPU, memory, system status) | `MetricsPanel` | status strip counters only | partial | |
| Models | `ModelsPanel` | none | missing | |
| Runners/machines, EAVS, deploy state, audit, auth policy | none | none | new | ADR-0067 §3 |

## Counts (first pass)

Of 59 rows: better 4, better? 3, same 2, partial 8, dead 1, missing 25, ? 14, new 2. This
first pass marks no row as **drop**: nothing is excluded until someone argues for it.

## Next steps

1. Resolve every `?` by running both UIs side by side at the three Studio views.
2. Fix the dead controls and identity (small, independent of the rest).
3. Turn each **missing** row into a child of `oqto-a9j4` or mark it **drop** with a reason.
