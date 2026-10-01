# AI Agent Workspace Frontend

A Vite + React experience for managing AI workspaces, live agent chat, and remote terminals hosted inside Podman containers. The UI now talks directly to an opencode server, streams events over SSE, browses files through the colocated file server, and embeds a Ghostty-powered terminal connected to the container PTY.

## Prerequisites

- **Node.js / Bun** – the project uses Bun scripts (`bun dev`, `bun lint`, etc.)
- **Running container** that exposes:
  - `opencode serve -p <PORT>` (HTTP + SSE)
  - WebSocket endpoint that bridges to the container PTY (e.g., `ttyd` or custom gateway)
  - File server capable of returning JSON trees + raw file content from the workspace root

## Environment Variables

Create a `.env.local` with the endpoints that match your Podman container:

```bash
VITE_OPENCODE_BASE_URL=http://localhost:4096
VITE_TERMINAL_WS_URL=ws://localhost:9090/ws
VITE_FILE_SERVER_URL=http://localhost:9000
VITE_CONTROL_PLANE_URL=http://localhost:8080
VITE_CADDY_BASE_URL=http://localhost
```

| Variable | Purpose |
| -------- | ------- |
| `VITE_OPENCODE_BASE_URL` | Base URL for the opencode REST + SSE API (`/session`, `/event`, etc.). |
| `VITE_TERMINAL_WS_URL` | WebSocket address that forwards raw PTY bytes. The Ghostty terminal streams directly to this socket. |
| `VITE_FILE_SERVER_URL` | HTTP server rooted at the same workspace folder the container starts in. Must expose `/tree?path=` and `/file?path=` helpers. |
| `VITE_CONTROL_PLANE_URL` | Control plane API base URL for sessions, auth, and proxy endpoints. |
| `VITE_CADDY_BASE_URL` | Optional Caddy proxy URL used for container routing in production. |
| `VITE_LOCAL_SPEECH_ISOLATION` | Set to `1` to opt the Vite dev server into COOP/COEP for local Moonshine recognition; review external frame/opener compatibility first. |

## Local Development

```bash
bun install
bun dev
```

The app runs on [http://localhost:3000](http://localhost:3000) and immediately begins calling the configured services. Use `bun lint` to run the Biome + oxlint suite.

The default route resolves to the sessions (chat) view so the most recent chat opens first.
The app uses a fixed viewport-height shell to avoid iOS PWA bottom inset drift after reopening, and the status bar avoids bottom safe-area padding.

## Browser dictation

The chat voice menu offers explicit local Moonshine or configured remote eaRS
recognition, with English/German selection. Local mode never uploads microphone
audio or silently falls back to a service. Previews stay separate from committed
composer text; Finish drains inference, Cancel discards provisional speech.

Local recognition requires HTTPS/localhost and cross-origin isolation. For dev:
`VITE_LOCAL_SPEECH_ISOLATION=1 bun dev`. Production operators must set COOP
`same-origin` and COEP `require-corp` and review cross-origin frames/resources.
A missing prerequisite is reported visibly; the published runtime has no
single-thread fallback. Model GET downloads are cached subject to browser eviction.

Dev/build scripts stage a checksum-pinned GitHub Moonshine WASM release into
`public/speech/` (gitignored), with upstream licenses. npm's same-version WASM is
older and incompatible with German/split frontends. The small reusable SDK source
is vendored; update it explicitly using `bun run sync:ears-browser-sdk`, then
`bun install` and `bun run sync:speech-runtime-assets`. Normal builds do not need
an eaRS checkout. See [ADR 0051](../docs/adr/0051-standalone-browser-recognition.md)
for the ownership boundary, exact proof and remaining validation gaps.

## Desktop (Tauri)

The frontend remains a standard web app. For a desktop shell, run Tauri:

```bash
bun run tauri:dev
```

Build a desktop bundle with:

```bash
bun run tauri:build
```

## PWA Notes (iOS)

The iOS PWA layout forces zero bottom safe-area padding to keep the status bar flush with the bottom edge during long sessions.

## Sidebar Notes

The session sidebar lists the search input above the Default Chat entry on both mobile and desktop layouts.
The mobile sidebar adds top safe-area padding so the header stays below the Dynamic Island.
Utility app headers (Dashboard, Settings, Admin) are centered on mobile.

## Testing with Podman

A ready-to-run container definition lives in `Dockerfile` with the companion launcher script at `scripts/entrypoint.sh`. It installs opencode, ttyd, and the file tooling described in the architecture notes.

1. Build the image:
   ```bash
   podman build -t ai-agent-workspace .
   ```
2. Run it and expose the default ports (opencode 4096, file server 9000, ttyd 9090). Mount a host workspace if desired:
   ```bash
   podman run --rm -it \
     -p 4096:4096 -p 9000:9000 -p 9090:9090 \
     -v $(pwd)/sandbox:/workspace \
     ai-agent-workspace
   ```
   Environment overrides such as `OPENCODE_PORT`, `FILE_SERVER_PORT`, or `TTYD_PORT` can be passed via `-e` flags.
3. Point the frontend env vars at `localhost` as shown above. The app will connect to the running container automatically.

## Project Structure Highlights

- `apps/` – pluggable app modules (Workspaces, Sessions, Admin) registered through `lib/app-registry`.
- `components/terminal/ghostty-terminal.tsx` – Ghostty + WebSocket terminal wrapper.
- `lib/opencode-client.ts` – Thin client for opencode REST/SSE workflows.
- `features/sessions/SessionScreen.tsx` – Sessions app screen composition and orchestration.
- `features/sessions/components/*` – File tree browser, terminal view, and preview surface wired to live services (file tree stays mounted while previews are open).
- `public/oqto_logo_banner_white.svg` – App logo used by the shell navigation (SVG icon).

Refer to the documents inside `history/` for deeper architecture notes on opencode and Ghostty integrations.
