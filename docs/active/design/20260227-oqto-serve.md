# oqto-serve: Agent Web Application Server

Status: PROPOSAL
Date: 2026-02-25

## Problem

Agents produce complex output that doesn't belong in a terminal: architecture diagrams, data dashboards, interactive forms, diff reviews, monitoring consoles, configuration wizards, project recaps. Today, the only options are:

1. **ASCII art in the chat** -- unreadable past trivial complexity
2. **A2UI surfaces** -- good for structured prompts (buttons, inputs, choices) but intentionally narrow; not designed for rich, freeform content
3. **Write an HTML file and tell the user to open it** -- breaks the workflow, loses the integration

Projects like [visual-explainer](https://github.com/nicobailon/visual-explainer) and [pi-design-deck](https://github.com/nicobailon/pi-design-deck) demonstrate the demand: agents generating self-contained HTML pages with real typography, Mermaid diagrams, dark/light themes, and interactive elements. But they work around the platform -- writing files to disk, opening a system browser, losing the connection to the Oqto workspace.

oqto-serve brings this capability *into* the platform. The agent writes HTML/CSS/TS in its workspace, calls one command, and the content appears in the user's Oqto frontend -- live, hot-reloading, properly styled, with access control handled automatically.

## What This Is

**oqto-serve is a general-purpose web application server that agents control.** The agent writes arbitrary HTML, CSS, and TypeScript files in a directory. `oqto-serve` serves them. The Oqto frontend renders them in an iframe panel. The agent has full creative freedom -- any JavaScript that runs in a browser works.

This is not a component library, not a declarative UI protocol, not a build system. It is a file server with TypeScript transpilation, file watching, and integration hooks into the Oqto backend and frontend.

**Use cases:**

| Category | Examples |
|----------|----------|
| **Visualization** | Architecture diagrams (Mermaid), data charts (Chart.js), flow visualizations, dependency graphs |
| **Dashboards** | System metrics, project status, test results, build progress, cost tracking |
| **Interactive tools** | Configuration wizards, data explorers, log viewers, JSON/YAML editors |
| **Reviews** | Visual diff reviews, plan audits, requirement matrices, design comparisons |
| **Presentations** | Design decks (a la pi-design-deck), multi-slide option previews, project recaps |
| **Throwaway utilities** | Quick data tables, one-off calculators, format converters |

**Relationship to A2UI:**

A2UI is a structured protocol for in-chat interactive prompts -- "pick one of these buttons," "fill in this form," "choose from this list." It lives inside the message stream and returns structured data to the agent. It is intentionally constrained.

oqto-serve is the other end of the spectrum: full web applications in a dedicated panel. No constraints on what the content can do. The agent writes code, the browser runs it. They serve completely different purposes and coexist naturally.

## Architecture

```
Agent (Pi)                     oqto-serve CLI                Backend (oqto)              Frontend
   |                                |                             |                          |
   |-- oqto-serve scaffold ------->| (creates files locally)     |                          |
   |-- oqto-serve start ./app ---->|                             |                          |
   |                                |-- POST /serve/start ------>|                          |
   |                                |<-- {port, serve_id} -------|                          |
   |                                |                             |-- WsEvent: serve.start ->|
   |                                |   [binds HTTP server]      |                          |-- opens ServeView
   |                                |   [starts file watcher]    |                          |   (iframe -> /serve/{id}/*)
   |                                |                             |                          |
   |-- (edits files) ------------->|   [watcher detects change]  |                          |
   |                                |-- POST /serve/reload ----->|-- WsEvent: serve.reload ->|
   |                                |                             |                          |-- iframe reloads
   |                                |                             |                          |
   |-- oqto-serve stop ----------->|-- POST /serve/stop -------->|                          |
   |                                |                             |-- WsEvent: serve.stop --->|
   |                                |                             |                          |-- closes ServeView
```

## Components

### 1. oqto-serve CLI (Rust binary)

Available to agents inside the sandbox. Acts as both CLI and HTTP server.

```
oqto-serve start [OPTIONS] <WORKDIR>    # Serve a directory, stay running
oqto-serve stop [--id <SERVE_ID>]       # Stop a running instance
oqto-serve list                          # List active instances for this session
oqto-serve scaffold <TEMPLATE> [DIR]     # Create project from template
```

#### `start` flow

```bash
oqto-serve start ./my-tool --title "Architecture Overview"
```

1. CLI contacts oqto backend (HTTP or Unix socket, same transport as oqtoctl, uses `$OQTO_SESSION_ID`)
2. Backend allocates a port from the dedicated range, returns `{ port, serve_id }`
3. CLI binds an HTTP server to `127.0.0.1:{port}`
4. Backend broadcasts `WsEvent::ServeStart` to the user's frontend
5. CLI starts a file watcher (`notify` crate) on the workdir
6. On file changes: invalidates transpile caches, sends `POST /serve/reload`
7. CLI stays running (foreground) until killed or `oqto-serve stop` is called

The agent starts this in the background and keeps editing files. Every save triggers a reload in the user's browser.

#### File serving

| Request path | Behavior |
|-------------|----------|
| `GET /` | Serves `index.html` |
| `GET /style.css` | Static file, `text/css` |
| `GET /main.ts` | **Transpiled to JS on-the-fly** via `swc`, served as `application/javascript` |
| `GET /lib/utils.ts` | Same transpilation, local `.ts` imports rewritten to `.js` |
| `GET /data.json` | Static file, `application/json` |
| Everything else | Standard static serving with correct MIME types |

TypeScript transpilation uses `swc` (Rust-native, compiled into the binary). No Node, no Bun, no external process. Transpiled output is cached in memory and invalidated on file change. For agent-generated content (single files or a handful of modules) this is instant.

Legacy note: CDN imports used to pass through untouched. As of the offline-first visual runtime work (`oqto-jty7`), default behavior for app surfaces is to run in `offline_strict` mode with local bundled assets and sanitizer/CSP enforcement. CDN passthrough is now considered transitional/developer-only behavior (`online_flexible`).

#### `scaffold` command

```bash
oqto-serve scaffold dashboard --output ./metrics
oqto-serve scaffold diagram --output ./arch
oqto-serve scaffold blank --output ./scratch
```

Templates are embedded in the binary (`include_dir!` or equivalent). They are a fast start, not a requirement. The agent can create everything from scratch, or take a template and completely rewrite it.

### 2. Port Allocation

Dedicated port range in oqto backend config:

```toml
[serve]
enabled = true
port_range_start = 50000
port_range_end = 50999
max_per_session = 5
```

Flow:

1. `POST /serve/start` with `session_id` -> backend finds lowest free port in range
2. Records `(serve_id, session_id, user_id, port, workdir, title)` in DB
3. Returns `{ port, serve_id }` to CLI
4. On stop/crash/session-end, frees the port

**Access control:** The oqto backend proxies all requests, same pattern as fileserver/ttyd/mmry/browser:

```
Frontend: GET /serve/{serve_id}/index.html
Backend:  look up serve_id -> (port, user_id)
          verify requesting user == user_id
          proxy to http://localhost:{port}/index.html
```

The serve instance binds `127.0.0.1` only. Never faces the network. No Caddy changes needed.

### 3. Frontend: ServeView

A new view in the session screen, alongside chat, files, terminal, browser, canvas.

**New ViewKey:** `"serve"` added to the type union.

**Behavior:**

- `WsEvent::ServeStart { serve_id, title, session_id }` -> store instance metadata, auto-switch to serve view
- `WsEvent::ServeReload { serve_id }` -> increment iframe key (triggers full page reload)
- `WsEvent::ServeStop { serve_id }` -> remove instance, switch to previous view

**Multiple instances:** If the agent starts several serve instances, the serve panel shows tabs at the top. Each tab renders its own iframe. The agent might run a dashboard AND a design deck simultaneously.

**iframe sandbox:** `sandbox="allow-scripts allow-forms allow-same-origin"` -- full JS execution, no parent frame navigation.

**Theme forwarding:** The parent sends `postMessage({ type: "oqto:theme", theme: "dark"|"light" })` to each iframe on theme change and on initial load. Templates include a listener that toggles a `.dark` class.

**oqtoctl integration:** `oqtoctl ui panel --view serve` and `oqtoctl ui view serve` work like existing views, so agents can programmatically open/close the serve panel.

### 4. Hot Reload

Simple and reliable:

1. File watcher (`notify`) detects change in workdir
2. If `.ts` file: invalidate transpile cache for that file
3. `POST /serve/reload` -> backend broadcasts `WsEvent::ServeReload`
4. Frontend increments iframe `key` -> full page reload

No HMR runtime, no WebSocket injection into served content, no build step. The iframe reloads completely. For agent-generated content this is fine -- pages load in milliseconds.

### 5. Templates and the Oqto Style

Templates ship as embedded files in the binary. Each template is a working example the agent can modify or learn from.

#### Design tokens (extracted from Oqto globals.css)

The base `style.css` that ships with every template:

```css
:root {
  --bg: #f7f8f7;
  --fg: #2a3330;
  --card: #eaedeb;
  --card-fg: #2a3330;
  --primary: #3ba77c;
  --primary-fg: #f7f8f7;
  --muted: #e0e3e1;
  --muted-fg: #6b7974;
  --border: #cdd1ce;
  --destructive: #b94a48;
  --code-bg: #f0f0f0;
  --code-fg: #1e1e1e;
  --font: "JetBrainsMono Nerd Font", ui-monospace, SFMono-Regular, monospace;
}

.dark {
  --bg: #222624;
  --fg: #c5ccc8;
  --card: #2d312f;
  --card-fg: #c5ccc8;
  --primary: #3ba77c;
  --primary-fg: #222624;
  --muted: #3a3f41;
  --muted-fg: #b2b9b5;
  --border: #2d312f;
  --destructive: #b94a48;
  --code-bg: #0f1412;
  --code-fg: #d5f0e4;
}
```

#### Style rules

- **No rounded corners.** `border-radius: 0` on everything. This is Oqto's signature.
- **Monospace everywhere.** JetBrainsMono Nerd Font, falling back to system monospace.
- **Thin borders.** 1px solid `var(--border)`.
- **No shadows in light mode, subtle in dark.** Flat, information-dense aesthetic.
- **Dense spacing.** Compact layouts. Small font sizes (12-13px base).
- **Color discipline.** Primary green (`#3ba77c`), neutral grays, no gratuitous color.

These match the Oqto frontend exactly, so served content looks native.

#### Template inventory

| Template | What it contains | CDN deps |
|----------|-----------------|----------|
| `blank` | Minimal `index.html` + `style.css`. Empty `main.ts`. | None |
| `dashboard` | Grid of stat cards, data table, status indicators | None |
| `diagram` | Mermaid diagram with zoom/pan, dark/light theme config | Mermaid (CDN) |
| `chart` | Responsive Chart.js chart with data loading | Chart.js (CDN) |
| `table` | Sortable, filterable data table with pagination | None |
| `form` | Multi-section form with validation, input groups | None |
| `deck` | Multi-slide presentation with navigation (inspired by pi-design-deck) | Mermaid, Prism.js (CDN) |
| `recap` | Project recap page with sections, KPI cards, status grid (inspired by visual-explainer) | Mermaid (CDN) |

Each template is a complete, working example. The agent reads it, understands the patterns, modifies it. Or ignores templates entirely and writes from scratch.

#### Theme sync snippet (in every template)

```html
<script>
  window.addEventListener("message", (e) => {
    if (e.data?.type === "oqto:theme") {
      document.documentElement.classList.toggle("dark", e.data.theme === "dark");
    }
  });
  if (window.matchMedia("(prefers-color-scheme: dark)").matches) {
    document.documentElement.classList.add("dark");
  }
</script>
```

#### Mermaid theming (for diagram/recap/deck templates)

Templates that use Mermaid configure it to match Oqto's palette:

```javascript
mermaid.initialize({
  theme: "base",
  themeVariables: {
    darkMode: document.documentElement.classList.contains("dark"),
    primaryColor: "#3ba77c",
    primaryTextColor: "#222624",
    primaryBorderColor: "#2d312f",
    lineColor: "#6b7974",
    fontFamily: "ui-monospace, monospace",
    fontSize: "13px",
  },
});
```

### 6. Sandbox Considerations

`oqto-serve` runs inside the same bwrap sandbox as the agent. It needs:

- **Loopback network** -- binds `127.0.0.1:{port}`. The `development` and `minimal` profiles allow this. The `strict` profile (`--unshare-net`) blocks it; CLI detects and errors clearly.
- **Workspace read access** -- always granted.
- **Backend communication** -- same mechanism as oqtoctl (`$OQTO_SESSION_ID` env var, HTTP to backend).

The port is pre-allocated by the backend. The CLI doesn't need to discover or negotiate ports.

### 7. Backend Changes

#### New API routes

```
POST   /serve/start       Allocate port, register instance, broadcast start event
POST   /serve/reload      Broadcast reload event
POST   /serve/stop        Deregister instance, broadcast stop event
GET    /serve/list         List active instances for current user
GET    /serve/{id}/*       Reverse proxy to localhost:{port}/* with auth check
POST   /serve/heartbeat   CLI pings every 30s to prove liveness
```

#### New DB migration

```sql
CREATE TABLE serve_instances (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  port INTEGER NOT NULL UNIQUE,
  workdir TEXT NOT NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'running',
  last_heartbeat INTEGER NOT NULL DEFAULT (unixepoch()),
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  FOREIGN KEY (session_id) REFERENCES sessions(id)
);
```

#### New WsEvent variants

```rust
WsEvent::ServeStart  { serve_id: String, title: String, session_id: String }
WsEvent::ServeReload { serve_id: String }
WsEvent::ServeStop   { serve_id: String }
```

#### Config section

```toml
[serve]
enabled = true
port_range_start = 50000
port_range_end = 50999
max_per_session = 5
heartbeat_timeout_seconds = 90
```

### 8. Lifecycle and Cleanup

| Event | Action |
|-------|--------|
| `oqto-serve stop` | CLI shuts down, `POST /serve/stop`, backend frees port, frontend closes tab |
| CLI process killed/crashes | Heartbeat stops; backend prunes after timeout, frees port |
| Session stops | Backend cleans up all `serve_instances WHERE session_id = ?` |
| Backend restarts | Prunes stale instances (matches `cleanup_on_startup` behavior) |

### 9. Agent Workflow Examples

#### Quick visualization

```bash
# Agent wants to show an architecture diagram
mkdir -p /tmp/arch
cat > /tmp/arch/index.html << 'EOF'
<!DOCTYPE html>
<html><head>
  <link rel="stylesheet" href="style.css">
  <script type="module" src="https://esm.sh/mermaid@11/dist/mermaid.min.js"></script>
</head><body>
  <div class="mermaid">
    graph TD
      A[Frontend] --> B[Backend/oqto]
      B --> C[Runner]
      C --> D[Pi Agent]
      C --> E[oqto-serve]
      B --> F[hstry]
  </div>
  <script>/* theme sync snippet */</script>
</body></html>
EOF
# ... plus style.css ...
oqto-serve start /tmp/arch --title "System Architecture"
```

#### Template-based dashboard

```bash
oqto-serve scaffold dashboard --output ./metrics
# Agent edits ./metrics/main.ts to fetch real data
# Agent edits ./metrics/index.html to add/remove cards
oqto-serve start ./metrics --title "Build Metrics"
# User sees dashboard in Oqto, agent keeps editing, dashboard hot-reloads
```

#### Design deck (inspired by pi-design-deck)

```bash
oqto-serve scaffold deck --output ./options
# Agent populates slides with code previews, Mermaid diagrams, mockups
oqto-serve start ./options --title "Auth Flow Options"
# User sees multi-slide deck in Oqto, picks an option
# Agent reads selection (via a simple fetch back to the serve instance or chat)
```

## Implementation Phases

### Phase 1: Core

- `oqto-serve` Rust binary: `start`, `stop`, `list`
- Static file serving with `swc` TypeScript transpilation
- Backend: port allocation, reverse proxy, DB table, WsEvents
- Frontend: ServeView component with iframe + reload logic

### Phase 2: Templates + Style

- `scaffold` command with embedded templates
- Base Oqto style CSS (design tokens, utility classes)
- Theme sync via postMessage
- Template library: blank, dashboard, diagram, chart, table, form

### Phase 3: Rich Templates + Multi-instance

- `deck` template (multi-slide presentations)
- `recap` template (project overview pages)
- Multiple concurrent instances with tabbed UI
- `oqtoctl ui panel --view serve` integration
- Heartbeat-based cleanup
- Status reporting in `oqtoctl session get`
