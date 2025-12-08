# Frontend Integration Notes: Opencode + Ghostty

## Overview

The frontend now talks directly to three services that live inside the same Podman container:

1. **Opencode API** – `NEXT_PUBLIC_OPENCODE_BASE_URL`
   - REST endpoints (`/session`, `/session/:id/messages`, `/session/:id/chat`)
   - Server-sent events stream at `/event`
   - Used to populate the Sessions app, stream live updates, and send chat input.

2. **File Server** – `NEXT_PUBLIC_FILE_SERVER_URL`
   - `/tree?path=` returns a JSON hierarchy of files/directories rooted at the workspace path used when starting opencode
   - `/file?path=` returns raw text for previews
   - Powers the FileTreeView + preview pane under the Sessions workspace view.

3. **PTY WebSocket (Ghostty)** – `NEXT_PUBLIC_TERMINAL_WS_URL`
   - Must expose a bidirectional WebSocket that pumps raw PTY bytes (e.g., `ttyd`, `websocketd`, or a custom bridge to the container shell)
   - The `GhosttyTerminal` React wrapper (see `components/terminal/ghostty-terminal.tsx`) instantiates the Ghostty WASM terminal, forwards keyboard input to the socket, and renders streamed bytes in real time.

## Data Flow

```
[Next.js UI]
   │
   ├── fetch -> ${OPENCODE}/session, /messages, /chat
   ├── SSE  -> ${OPENCODE}/event (updates sessions + chat transcript)
   ├── fetch -> ${FILE_SERVER}/tree + /file (file browser + preview)
   └── ws    -> ${TERMINAL_WS} (Ghostty terminal <-> container PTY)
```

## Usage Checklist

1. Start the container and run `opencode serve -p 4096` (or any port – match the env var).
2. Expose a file API rooted at the same workspace directory the agent operates in.
3. Launch a PTY bridge (e.g., `ttyd -p 9090 bash`).
4. Set the three `NEXT_PUBLIC_*` variables and run `bun dev`.

The Sessions app will now:
- List opencode sessions retrieved from `/session`
- Stream updates through SSE and refresh the transcript on new `message.*` events
- Allow users to send chat messages via `/session/:id/chat`
- Browse/preview workspace files through the file server
- Attach to the live PTY using Ghostty and the WebSocket bridge
