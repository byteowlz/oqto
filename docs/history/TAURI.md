# Tauri Desktop & Mobile App Implementation Plan

Wrap Oqto frontend in a Tauri v2 app for desktop (macOS, Windows, Linux) and mobile (iOS, Android) by enhancing the Rust backend to serve everything from a single origin.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│              Tauri App (Desktop & Mobile)                   │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                 Oqto Backend (Axum)                   │  │
│  │                                                       │  │
│  │  /                    → Static files (frontend dist)  │  │
│  │  /api/*               → REST API (existing)           │  │
│  │  /session/:id/term    → WS proxy → ttyd (existing)    │  │
│  │  /session/:id/code/*  → SSE proxy → OpenCode (existing)│  │
│  │  /api/voice/stt       → WS proxy → eaRS               │  │
│  │  /api/voice/tts       → WS proxy → kokorox            │  │
│  │                                                       │  │
│  └───────────────────────────────────────────────────────┘  │
│                            │                                │
│  ┌─────────────────────────┴─────────────────────────────┐  │
│  │                    Tauri Shell                        │  │
│  │   - Spawns backend on app start                       │  │
│  │   - Points webview to http://localhost:8080           │  │
│  │   - Native window, menus, system tray (desktop)       │  │
│  │   - Native navigation, gestures (mobile)              │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Deployment Modes

| Mode | How it works |
|------|--------------|
| **Webapp** | Backend serves static frontend + API. Single binary, no Next.js server needed. |
| **Tauri Desktop** | Backend runs locally, Tauri webview loads from backend URL |
| **Tauri Mobile** | App connects to remote backend URL configured by user |

## Target Platforms

| Platform | Status | Notes |
|----------|--------|-------|
| Web | Primary | Any modern browser, backend serves frontend |
| macOS | Primary | Intel & Apple Silicon |
| Windows | Primary | x64, arm64 |
| Linux | Primary | x64, AppImage/deb |
| iOS | Secondary | Requires Xcode, Apple Developer account |
| Android | Secondary | Requires Android Studio, NDK |

## Tasks

### Phase 1: Backend Enhancements

1. **Add static file serving** - Use `tower_http::services::ServeDir` to serve frontend static export from `/`, with SPA fallback to `index.html`. Enables single-binary deployment and webapp mode without Next.js server.

2. **Add voice WebSocket proxies** - Bidirectional WS proxy for eaRS (`/api/voice/stt`) and kokorox (`/api/voice/tts`). Simple passthrough, no protocol translation needed.

### Phase 2: Frontend Changes

3. **Configure static export** - Set `output: 'export'` in next.config.ts, disable image optimization

4. **Remove server-side auth** - Delete middleware.ts, implement client-side auth guard in app layout

5. **Update voice URL resolution** - Detect Tauri/proxied mode, use relative WebSocket paths (`/api/voice/stt`, `/api/voice/tts`) instead of direct URLs

6. **Add mobile-responsive UI** - Ensure touch targets, safe areas, and gestures work on mobile

7. **Add backend URL configuration** - Server URL field on login form, persisted to localStorage, with connection status indicator

### Phase 3: Tauri Integration

7. **Create Tauri v2 project** - Initialize `src-tauri/` with Cargo.toml, tauri.conf.json, capabilities

8. **Implement Tauri main with backend startup** - Start Axum backend on launch, configure webview

9. **Configure mobile targets** - Set up iOS and Android build configurations

10. **Add platform-specific features** - Desktop: window management, tray. Mobile: haptics, safe areas

## Implementation Notes

### Static File Serving (Backend)

```rust
use tower_http::services::{ServeDir, ServeFile};

let spa = ServeDir::new("dist")
    .not_found_service(ServeFile::new("dist/index.html"));

Router::new()
    .nest("/api", api_routes)
    .merge(ws_routes)
    .fallback_service(spa)
```

### Voice WebSocket Proxy (Backend)

```rust
async fn proxy_voice_ws(client: WebSocket, backend_url: String) {
    let backend = connect_async(&backend_url).await?;
    // Bidirectional forward - no protocol translation
    tokio::select! {
        _ = forward(client_read, backend_write) => {},
        _ = forward(backend_read, client_write) => {},
    }
}
```

### Frontend Static Export

```typescript
// next.config.ts
const nextConfig: NextConfig = {
  output: 'export',
  images: { unoptimized: true },
  // Remove rewrites - backend handles routing
};
```

### Voice URL Detection (Frontend)

```typescript
function getVoiceWsUrl(path: string): string {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${proto}//${location.host}${path}`;
}
// Use: getVoiceWsUrl('/api/voice/stt')
```

### Backend URL Configuration (Frontend)

```typescript
// lib/backend-url.ts
const STORAGE_KEY = 'octo_backend_url';

export function getBackendUrl(): string {
  if (typeof window === 'undefined') return '';
  return localStorage.getItem(STORAGE_KEY) || window.location.origin;
}

export function setBackendUrl(url: string): void {
  localStorage.setItem(STORAGE_KEY, url);
}

export function isRemoteBackend(): boolean {
  const stored = localStorage.getItem(STORAGE_KEY);
  return !!stored && stored !== window.location.origin;
}
```

```typescript
// Login form - add server URL field
<Input
  label="Server URL"
  placeholder="https://oqto.example.com"
  value={serverUrl}
  onChange={setServerUrl}
/>
<ConnectionStatus url={serverUrl} /> // Shows connected/error state
```

### Tauri Mobile Commands

```bash
# iOS
cargo tauri ios init
cargo tauri ios dev
cargo tauri ios build

# Android  
cargo tauri android init
cargo tauri android dev
cargo tauri android build
```

## Dependencies

- Tauri CLI v2: `cargo install tauri-cli --version "^2.0"`
- Frontend: `@tauri-apps/api@^2`, `@tauri-apps/cli@^2`
- iOS: Xcode 15+, iOS 13+ target
- Android: Android Studio, NDK 25+, API 24+ target

## Mobile Considerations

### Voice Mode on Mobile
- iOS: Requires microphone permission in Info.plist
- Android: Requires RECORD_AUDIO permission in AndroidManifest.xml
- Both: Handle audio session/focus for STT/TTS

### Terminal on Mobile
- Touch keyboard integration
- Consider read-only mode or simplified shell UI
- Handle virtual keyboard resize events

### Offline/Background
- Mobile apps may be suspended - handle reconnection gracefully
- Consider local caching for chat history

## Out of Scope (Future)

- Native file dialogs for workspace selection
- Deep OS integration (protocol handlers, file associations)
- Code signing and notarization
- Auto-update infrastructure
- Push notifications for mobile
