# Inline HTML Apps (Workspace Apps)

Status: PROPOSAL
Date: 2026-03-09

## Problem

Agents and users need a way to run self-contained HTML applications directly within the Oqto frontend -- dashboards, forms, data viewers, interactive tools -- without spinning up a separate server process. The full `oqto-serve` infrastructure (port allocation, reverse proxy, heartbeats) is overkill for single-file apps that just need an iframe and file access.

## Solution

Render workspace HTML files as live apps in `<iframe srcdoc>` panels within the session screen. The frontend reads file content via the existing mux-files channel, injects the `window.apphost` bridge, and displays it as a new tab alongside chat/files/terminal. No backend changes required.

## Relationship to oqto-serve

This is **Phase 0** of the app story. It covers single-file HTML apps using `srcdoc` iframes with `postMessage`-bridged file access. `oqto-serve` (oqto-14b1) remains the solution for multi-file apps with relative imports, TypeScript transpilation, and CDN passthrough.

| Capability | Inline Apps (this) | oqto-serve |
|---|---|---|
| Single-file HTML | Yes | Yes |
| Multi-file with imports | No | Yes |
| TypeScript | No (unless pre-bundled) | Yes (swc) |
| Backend changes | None | Port alloc, proxy, heartbeat |
| File read/write | Via apphost bridge | Via apphost bridge |
| Hot reload | File watcher events | File watcher + CLI |

## Architecture

```
Agent writes HTML file in workspace
  -> Frontend detects file (context menu or pinned config)
  -> readFileMux() fetches content
  -> Frontend injects apphost shim + CSS vars
  -> <iframe srcdoc={injectedHtml}> renders in AppView tab

App calls apphost.writeFile("data.json", content)
  -> postMessage to parent
  -> parent calls writeFileMux()
  -> file written to workspace

Agent reads/writes same files
  -> bidirectional data flow via shared workspace files
```

## Frontend Changes

### 1. New ViewKey: `"app"`

Add `"app"` to the `ViewKey` union in `SessionScreen.tsx`. Supports multiple open apps via an internal tab bar.

```typescript
type ViewKey = "chat" | "overview" | "tasks" | "files" | "canvas"
             | "memories" | "terminal" | "browser" | "settings"
             | "app";  // NEW
```

### 2. AppView Component

`features/sessions/components/AppView.tsx`

State per open app:
```typescript
interface AppTab {
  id: string;           // unique tab id
  filePath: string;     // workspace-relative path to HTML file
  title: string;        // display name (filename or <title>)
  content: string;      // last loaded HTML content
  pinned: boolean;      // from workspace config
}
```

Rendering:
- Tab bar at top when multiple apps open
- `<iframe srcdoc={injectApphost(content)} sandbox="allow-scripts allow-forms allow-modals allow-popups">` filling the panel
- Refresh button per tab (re-reads file)
- Close button per tab (unless pinned)
- Auto-reload when file watcher fires `file_changed` for the app's path

### 3. Apphost Bridge Injection

Before setting `srcdoc`, the frontend wraps the HTML content:

```typescript
function injectApphost(html: string, theme: string, workspacePath: string): string {
  const shim = `
<script>
(function() {
  const pending = new Map();
  let msgId = 0;

  function request(type, payload) {
    return new Promise((resolve, reject) => {
      const id = ++msgId;
      pending.set(id, { resolve, reject });
      parent.postMessage({ source: 'oqto-app', id, type, ...payload }, '*');
    });
  }

  window.addEventListener('message', (e) => {
    if (e.data?.source !== 'oqto-host') return;
    if (e.data.id && pending.has(e.data.id)) {
      const { resolve, reject } = pending.get(e.data.id);
      pending.delete(e.data.id);
      if (e.data.error) reject(new Error(e.data.error));
      else resolve(e.data.result);
      return;
    }
    if (e.data.type === 'theme_change') {
      window.apphost.theme = e.data.theme;
      themeCallbacks.forEach(cb => cb(e.data.theme));
      applyThemeVars(e.data.vars);
    }
    if (e.data.type === 'state_update') {
      messageCallbacks.forEach(cb => cb(e.data.data));
    }
  });

  const themeCallbacks = new Set();
  const messageCallbacks = new Set();

  function applyThemeVars(vars) {
    for (const [k, v] of Object.entries(vars || {})) {
      document.documentElement.style.setProperty(k, v);
    }
  }

  window.apphost = {
    host: 'oqto',
    theme: '${theme}',
    onThemeChange(cb) { themeCallbacks.add(cb); return () => themeCallbacks.delete(cb); },
    send(data) { parent.postMessage({ source: 'oqto-app', type: 'app_message', data }, '*'); },
    onMessage(cb) { messageCallbacks.add(cb); return () => messageCallbacks.delete(cb); },
    readFile(path) { return request('read_file', { path }); },
    writeFile(path, data) { return request('write_file', { path, data }); },
    saveState(key, value) { return request('save_state', { key, value }); },
    loadState(key) { return request('load_state', { key }); },
  };

  // Apply initial theme
  parent.postMessage({ source: 'oqto-app', type: 'ready' }, '*');
})();
</script>
<style>
  :root {
    --app-bg: var(--background, #0f1210);
    --app-fg: var(--foreground, #e0e4e1);
    --app-card: var(--card, #181b1a);
    --app-card-fg: var(--card-foreground, #e0e4e1);
    --app-primary: var(--primary, #3ba77c);
    --app-primary-fg: var(--primary-foreground, #ffffff);
    --app-muted: var(--muted, #232826);
    --app-muted-fg: var(--muted-foreground, #9ca89e);
    --app-border: var(--border, #2a2f2c);
    --app-destructive: var(--destructive, #e74c3c);
    --app-success: #3ba77c;
    --app-warning: #f39c12;
    --app-info: #3498db;
    --app-font: ui-sans-serif, system-ui, sans-serif;
  }
  body {
    background: var(--app-bg);
    color: var(--app-fg);
    font-family: var(--app-font);
    margin: 0;
  }
</style>`;

  // Inject before </head> or at start of document
  if (html.includes('</head>')) {
    return html.replace('</head>', shim + '</head>');
  }
  return shim + html;
}
```

### 4. Host-Side Message Handler

The parent frame listens for postMessage from the iframe and routes requests:

```typescript
window.addEventListener('message', async (e) => {
  if (e.data?.source !== 'oqto-app') return;

  const iframe = appIframeRef.current;
  if (!iframe || e.source !== iframe.contentWindow) return;

  switch (e.data.type) {
    case 'read_file': {
      const result = await readFileMux(workspacePath, e.data.path);
      iframe.contentWindow.postMessage({
        source: 'oqto-host', id: e.data.id,
        result: new TextDecoder().decode(result),
      }, '*');
      break;
    }
    case 'write_file': {
      const content = new TextEncoder().encode(
        typeof e.data.data === 'string' ? e.data.data : JSON.stringify(e.data.data)
      );
      await writeFileMux(workspacePath, e.data.path, content.buffer, true);
      iframe.contentWindow.postMessage({
        source: 'oqto-host', id: e.data.id, result: true,
      }, '*');
      break;
    }
    case 'save_state': {
      await writeFileMux(workspacePath,
        `.oqto/app-state/${e.data.key}.json`,
        new TextEncoder().encode(JSON.stringify(e.data.value)).buffer, true);
      iframe.contentWindow.postMessage({
        source: 'oqto-host', id: e.data.id, result: true,
      }, '*');
      break;
    }
    case 'load_state': {
      try {
        const raw = await readFileMux(workspacePath, `.oqto/app-state/${e.data.key}.json`);
        const value = JSON.parse(new TextDecoder().decode(raw));
        iframe.contentWindow.postMessage({
          source: 'oqto-host', id: e.data.id, result: value,
        }, '*');
      } catch {
        iframe.contentWindow.postMessage({
          source: 'oqto-host', id: e.data.id, result: null,
        }, '*');
      }
      break;
    }
    case 'app_message': {
      // Phase 2: route to agent session
      break;
    }
  }
});
```

### 5. File Context Menu

Add "Open as App" to `FileContextMenu` for `.html` files:

```typescript
{isHtml && onOpenAsApp && (
  <>
    <ContextMenuItem onClick={() => onOpenAsApp(node.path)}>
      <AppWindow className="w-4 h-4 mr-2" />
      Open as App
    </ContextMenuItem>
    <ContextMenuSeparator />
  </>
)}
```

### 6. Workspace Pinned Apps

In workspace settings (or `.oqto/apps.json`):

```json
{
  "apps": [
    {
      "path": "tools/dashboard.html",
      "title": "Project Dashboard",
      "autoOpen": true
    },
    {
      "path": "tools/data-viewer.html",
      "title": "Data Viewer"
    }
  ]
}
```

Apps with `autoOpen: true` open as tabs when the session starts. Others appear in a quick-launch menu.

### 7. Auto-Reload via File Watcher

The mux-files channel already supports `watch_files` / `file_changed` events. When a `file_changed` event matches an open app's path:

1. Re-read the file via `readFileMux()`
2. Re-inject apphost shim
3. Update `srcdoc` (this reloads the iframe)

For state preservation across reloads, apps should use `apphost.saveState()` / `apphost.loadState()` which persists to `.oqto/app-state/`.

## Agent-to-App Communication (File-Based, Phase 1)

Bidirectional agent<->app data flow uses shared workspace files:

```
Agent writes .oqto/app-data/dashboard/metrics.json
  -> file_changed event fires
  -> frontend reads file, postMessages into iframe
  -> app's onMessage callback receives data

User clicks button in app
  -> app calls apphost.writeFile(".oqto/app-data/dashboard/selection.json", data)
  -> writeFileMux writes to workspace
  -> agent reads file on next tool call (or watches with inotify)
```

Convention:
- `.oqto/app-data/<app-name>/` -- shared data files
- `.oqto/app-state/<key>.json` -- per-app persistent state (survives reload)

## Agent-to-App Communication (Message Channel, Phase 2)

Add `app_message` to the canonical protocol:

```
Agent sends: {"type": "app_message", "app_id": "dashboard", "data": {...}}
  -> runner broadcasts canonical event
  -> frontend routes to correct iframe via postMessage
  -> app's onMessage callback fires

User action in app triggers apphost.send(data)
  -> parent receives postMessage
  -> frontend sends command via WebSocket: {"type": "app_input", "app_id": "dashboard", "data": {...}}
  -> runner injects into Pi stdin as tool result or special prompt
```

This requires runner + canonical protocol additions. Deferred to Phase 2.

## Templates

Ship starter templates that agents can copy into workspaces. Templates use `--app-*` CSS variables for automatic theme integration.

Location: `templates/apps/`

### Starter Templates

| Template | Description |
|---|---|
| `blank.html` | Minimal skeleton with apphost + theme vars |
| `dashboard.html` | Card grid with metrics, uses readFile for data |
| `form.html` | Input form that writes results via writeFile |
| `data-table.html` | Sortable table that reads JSON data files |
| `kanban.html` | Drag-and-drop board with saveState persistence |
| `markdown-viewer.html` | Renders markdown files from workspace |

All templates are single-file, self-contained (inline CSS/JS), and use only `--app-*` variables for styling. They include marked.js/mermaid via CDN `<script>` tags where needed.

### Template API Documentation

Each template includes a comment block documenting the apphost API:

```html
<!--
  Oqto Workspace App Template
  
  Available API (window.apphost):
    .theme              - "dark" | "light"
    .onThemeChange(cb)  - subscribe to theme changes
    .readFile(path)     - read workspace file (returns Promise<string>)
    .writeFile(path, d) - write workspace file (returns Promise<void>)
    .saveState(key, v)  - persist app state across reloads
    .loadState(key)     - load persisted state (returns Promise<any>)
    .send(data)         - send message to agent (Phase 2)
    .onMessage(cb)      - receive messages from agent (Phase 2)
  
  CSS Variables: --app-bg, --app-fg, --app-card, --app-card-fg,
    --app-primary, --app-primary-fg, --app-muted, --app-muted-fg,
    --app-border, --app-destructive, --app-success, --app-warning,
    --app-info, --app-font
-->
```

## Security

- `sandbox="allow-scripts allow-forms allow-modals allow-popups"` -- no `allow-same-origin` (prevents iframe from accessing parent cookies/storage)
- File access is scoped to the workspace path -- the host-side handler validates paths are within bounds
- `writeFile` paths are sanitized (no `..` traversal, no absolute paths)
- State files go to `.oqto/app-state/` only (not arbitrary locations for saveState)
- Phase 2 message channel requires explicit opt-in per session

## Implementation Order

1. **AppView component** with srcdoc rendering and apphost shim injection
2. **ViewKey + tab integration** in SessionScreen
3. **Context menu** "Open as App" for .html files in FileTreeView
4. **File read/write bridge** (postMessage <-> mux-files)
5. **Auto-reload** via file watcher events
6. **Workspace pinned apps** config (`.oqto/apps.json`)
7. **Templates** in `templates/apps/`
8. **saveState/loadState** persistence
