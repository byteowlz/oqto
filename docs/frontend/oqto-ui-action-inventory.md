# OqtoUI action inventory (verb audit)

Status: working document for `oqto-qty2`, feeding the Action registry required by
[ADR-0040](../adr/0040-oqto-customizations-lua-configurable-ui.md). This is not the API. It is the
evidence-backed list of user-visible semantic verbs across the legacy UI, OqtoUI, and the
corner-mode probes, used to pressure-test the naming scheme and parameter shapes before the first
`stable` registry entries are frozen.

Sources audited: `frontend/messages/en.json` (every user-facing label), `frontend/lib/api/*`
clients, the legacy command palette, session/file context menus, permission and voice flows,
`artifacts/oqto-ui/corner-mode-concepts/` interactions, and the drag-and-drop design from the
customization MVP discussion.

## Naming conventions under test

1. **`noun.verb`, singular noun, camelCase verb.** `chat.send`, `session.fork`, `layout.dropResource`.
   Nouns are domain nouns from `CONTEXT.md`, not component names.
2. **Explicit targets, no ambient state in the contract.** Every action takes its target ID
   (`viewId`, `sessionId`, `path`). Hosts may offer the sugar alias `"focused"`, which the shell
   resolves to a concrete ID *before* dispatch; the executed action always records concrete IDs.
3. **Queries are Providers, not Actions.** List/search/filter/sort/get verbs are excluded here.
   `search`, `filter`, and `sort` are picker parameters over Providers. This removes an entire
   class of would-be actions and is the single most load-bearing boundary in this audit.
4. **State-setting over toggling.** Actions set explicit values (`view.setCollapsed { collapsed }`).
   Omitting the value means toggle; bindings that want a toggle simply omit it. No separate
   `*.toggle` verbs.
5. **Destructive is a flag, not a verb.** `session.delete` carries `destructive: true` in the
   registry; confirmation UX is shell policy. There are no `confirmDelete`-style verbs.
6. **Lifecycle vs conversation.** `session.*` is identity/lifecycle (create, stop, delete, fork).
   `chat.*` is conversation acts on a bound Chat View (send, cancel, copy). `agent.*` is harness
   process control (restart), distinct from both.
7. **UI-state verbs are actions too** (`view.focus`, `shell.openPalette`) — bindable, but never
   durable facts; executing them writes no store.

## Inventory

Tier legend: **S** = propose `stable` for the MVP freeze · **E** = `experimental` ·
**I** = `internal` until a preset needs it. Params show shape, not full schema.

### session (identity and lifecycle)

| Action | Params | Tier | Evidence |
| --- | --- | --- | --- |
| `session.create` | `workDirId, agent?, model?` | S | New Session button, `createWorkspaceSession` |
| `session.open` | `sessionId, viewId?` | S | list click, quick-switch, MRU fan |
| `session.stop` | `sessionId` | S | session actions menu |
| `session.start` | `sessionId` | S | session actions, `resumeWorkspaceSession` |
| `session.restart` | `sessionId` | E | session actions, `restartWorkspaceSession` |
| `session.delete` | `sessionId` (destructive) | S | delete dialog, bulk delete |
| `session.rename` | `sessionId, title` | S | rename dialog |
| `session.fork` | `sessionId, messageId?` | E | "Fork here", probe radial |
| `session.pin` | `sessionId, pinned?` | E | pin/unpin |
| `session.copyId` | `sessionId` | E | "Copy Temp ID" |
| `session.export` | `sessionId, format` | E | "Download session as markdown" |
| `session.upgrade` | `sessionId` | I | `upgradeWorkspaceSession` |

### chat (conversation acts on a Chat View)

| Action | Params | Tier | Evidence |
| --- | --- | --- | --- |
| `chat.send` | `viewId, text, attachments?` | S | composer, probe send corner |
| `chat.cancel` | `viewId` | S | "Stop agent" |
| `chat.retry` | `viewId, messageId?` | E | retry affordance |
| `chat.attach` | `viewId, resource` | S | attach button, probe radial |
| `chat.copyMessage` | `messageId` | S | message copy |
| `chat.copyAll` | `viewId` | E | "Copy all" |
| `chat.jumpToMessage` | `viewId, messageId` | E | quick-scroll rail |
| `chat.setModel` | `viewId, modelId` | S | model picker (queued when busy) |
| `chat.setReasoning` | `viewId, level` | E | reasoning level selector |
| `chat.setVerbosity` | `viewId, level` | E | chat verbosity setting |

### agent (harness process control)

| Action | Params | Tier | Evidence |
| --- | --- | --- | --- |
| `agent.restart` | `sessionId` | E | "Restart agent" |
| `agent.answer` | `requestId, answers` | E | question dialogs |
| `agent.grant` | `requestId, scope: once\|session\|always` | E | permission dialogs (kernel-gated) |
| `agent.deny` | `requestId, always?` | E | permission dialogs (kernel-gated) |

`agent.grant`/`agent.deny` execute in the trusted kernel; they are listed because they are
user-visible verbs, but bindings may only *surface* the pending request, never auto-answer it.

### view / layout (arrangement)

| Action | Params | Tier | Evidence |
| --- | --- | --- | --- |
| `view.focus` | `viewId` | S | MVP keybindings |
| `view.close` | `viewId` | S | tab close |
| `view.pin` | `viewId, pinned?` | E | pinned work-area tabs |
| `view.setCollapsed` | `viewId, collapsed?` | E | nav rail collapse, side panel toggle |
| `layout.open` | `viewKind, target, zone?, relativeTo?` | S | "Open in new pane / as tab / split right" |
| `layout.dropResource` | `resource, targetViewId, zone` | S | drag-and-drop design |
| `layout.move` | `viewId, targetViewId, zone` | E | tab drag |
| `layout.resize` | `nodeId, fraction` | E | split drag |
| `layout.applyPreset` | `presetId, preserveBindings` | S | preset switch/reset |
| `layout.saveAsPreset` | `name` | E | "Save current layout as preset" |

### files (work-directory file operations)

| Action | Params | Tier | Evidence |
| --- | --- | --- | --- |
| `files.open` | `path, viewKind?` | S | open / "Open in Editor" / preview |
| `files.create` | `path, kind: file\|folder` | E | New File / New Folder |
| `files.rename` | `path, newName` | E | rename |
| `files.delete` | `path` (destructive) | E | delete |
| `files.upload` | `targetDir, payloadRef` | E | upload |
| `files.download` | `path` | E | download |
| `files.copyPath` | `path` | E | context menu |
| `files.setRoot` | `viewId, path` | E | home/parent navigation |
| `files.setViewMode` | `viewId, mode: tree\|list\|grid` | E | view switcher |

### workdir / workspace (tenant navigation)

| Action | Params | Tier | Evidence |
| --- | --- | --- | --- |
| `workdir.open` | `workDirId` | S | ribbon, navigator |
| `workdir.create` | `workspaceId, template?, path, shared?` | E | New project dialog |
| `workdir.rename` | `workDirId, name` | E | rename project |
| `workdir.delete` | `workDirId` (destructive) | E | delete project |
| `workdir.pin` | `workDirId, pinned?` | E | pin/unpin |
| `workspace.switch` | `workspaceId` | S | tenant carousel (probe), workspace picker |
| `workspace.configure` | `workspaceId` | I | workspace settings pane |

### shell (host chrome, pickers, palette)

| Action | Params | Tier | Evidence |
| --- | --- | --- | --- |
| `shell.openPalette` | `pickerId?` | S | command palette (Ctrl+K) |
| `shell.openPicker` | `pickerId, query?` | S | navigator, model sheet, MRU, tool wheel |
| `shell.openMenu` | `menuId, anchor?` | E | radials/carousels (probe) |
| `shell.setLanguage` | `locale` | E | palette DE/EN |
| `shell.navigate` | `route` | E | settings/admin/dashboard destinations |
| `shell.logout` | — | E | sign out (kernel-executed) |

### appearance / config

| Action | Params | Tier | Evidence |
| --- | --- | --- | --- |
| `appearance.setScheme` | `schemeId` | S | theme picker |
| `appearance.setMode` | `mode: light\|dark\|system` | E | palette light/dark |
| `appearance.setDial` | `dial, value` | E | radius/shadow/blur/density/font customizer |
| `appearance.setRole` | `target, role` | E | per-token overrides |
| `appearance.reset` | — | E | "Reset to scheme defaults" |
| `config.reload` | — | S | MVP CLI/command |
| `config.applyPreset` | `presetId` | E | preset switching (config-level) |

### terminal / voice / gallery / tasks

| Action | Params | Tier | Evidence |
| --- | --- | --- | --- |
| `terminal.open` | `workDirId, viewId?` | E | Terminal tool |
| `terminal.reconnect` | `viewId` | E | reconnect button |
| `voice.startDictation` | `viewId` | E | dictation mode |
| `voice.startConversation` | `viewId` | E | conversation mode |
| `voice.stop` | `viewId` | E | stop |
| `gallery.openResource` | `resourceId, viewId?` | E | Gallery tiles |
| `tasks.openPlan` | `sessionId` | E | task progress expander |

### admin / account (kernel-owned, listed for completeness)

`admin.killSession`, `admin.forceStop`, `admin.manageUsers`, `admin.manageInvites`,
`account.changePassword`, `account.createApiKey`, `account.revokeApiKey`,
`account.connectProvider`, `account.disconnectProvider` — all **internal**: they exist as verbs,
run in trusted surfaces, and are deliberately not exposed to the config plane. Listing them here
records the decision; the registry marks them `internal` so exposure is a diff, not an accident.

## Counts

~80 verbs total: 20 proposed `stable` (matching the MVP scope), ~45 `experimental`,
~15 `internal`. Queries removed to Providers: session lists, message search/FTS, model catalog,
file listings, workdir/workspace lists, memories, agents, gallery resources, admin metrics.

## Naming decisions surfaced by the audit

1. **`session.open` vs `chat.*`:** opening is a Session-identity act that *binds a View*; it lives
   in `session.*` with an optional `viewId` destination. Everything after binding is `chat.*`.
2. **"Stop agent" is `chat.cancel`, not `agent.stop`:** it cancels the turn on one View's
   conversation. `agent.restart` is the only process-level verb users actually have.
3. **Model switching is per-View (`chat.setModel`), not global**, matching the queued-while-busy
   semantics the legacy UI already implements.
4. **Legacy "project" = `workdir`.** The i18n namespace `projects.*` maps onto `workdir.*`;
   the registry uses domain language, translations keep the user-facing word.
5. **Pickers subsume many buttons.** Model select, tenant switch, MRU, tool wheel, navigator,
   command palette are all `shell.openPicker`/`shell.openPalette` over different Providers with
   different Menu presentations — one mechanism, config-composable, exactly as ADR-0040 intends.
6. **`layout.open` unifies "Open in new pane/as tab/split X"** with a `zone` param instead of one
   verb per direction; `layout.dropResource` is its drag-initiated twin and shares the zone type.
7. **Voice is a mode of the composer**, parameterized per View, not a global toggle.
8. **No `ui.*` namespace.** Everything tempted to live there fit `view.*`, `shell.*`, or
   `appearance.*`; if a future verb doesn't, that is a smell worth a design pass, not a bucket.

## Open questions for the registry design

- Does `"focused"` sugar resolve in the binding layer or the dispatcher? (Proposal: binding layer,
  so traces always carry concrete IDs.)
- Are bulk operations (`session.delete` with many IDs) one action with `ids[]` or a shell-side
  loop? (Proposal: `ids[]`, single confirmation, single audit record.)
- Do `agent.grant`/`agent.deny` appear in the registry at all, or only in a kernel-verb appendix?
  Exposure risk says appendix; discoverability says registry-with-`internal`. (Proposal: registry,
  `internal`, non-promotable without an ADR.)
- `session.export` format list — provider-driven or enum?

## Next step

Freeze the 20 `stable` entries as the first registry content for the customization MVP
(`oqto-nq3b`), then let shell migration promote the rest deliberately.
