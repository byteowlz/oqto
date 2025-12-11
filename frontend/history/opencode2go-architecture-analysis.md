# opencode2go Architecture Analysis

> Analysis Date: December 9, 2025
> Source: ~/byteowlz/opencode2go
> Purpose: Understanding architecture for adapting to multi-user per-container system

## Overview

opencode2go is a Tauri-based GUI client for the [opencode](https://opencode.ai) terminal AI agent. It provides a cross-platform (Mac, Windows, Linux, iOS, Android) interface to communicate with opencode servers running in various locations.

**Key Point**: This application is a **client** that connects to opencode servers - it does NOT manage podman containers directly. The opencode servers it connects to may be running inside containers, but the container management is external to this application.

---

## 1. Communication Protocol

### HTTP REST API
The primary communication is via HTTP REST APIs to the opencode server:

**Core Endpoints** (from `src/services/opencode.ts`):

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/app` | GET | Health check / connection test |
| `/app/providers` | GET | List available AI providers and models |
| `/app/modes` | GET | List available operational modes (build, plan) |
| `/session` | GET | List all sessions |
| `/session` | POST | Create new session |
| `/session/:id` | DELETE | Delete a session |
| `/session/:id/chat` | POST | Send message to session |
| `/session/:id/messages` | GET | Get messages for session |
| `/session/:id/children` | GET | Get child sessions |
| `/agent` | GET | List available agents |
| `/config/permission` | POST | Update tool permissions |
| `/tui/show-toast` | POST | Display toast notifications |
| `/event` | GET (SSE) | Real-time event stream |

### Server-Sent Events (SSE)
Real-time updates are received via SSE on `/event`:

```typescript
// From src/services/opencode.ts, lines 477-550
subscribeToEvents(onEvent: (event: any) => void): () => void {
    // Uses Tauri's native SSE implementation to avoid CORS
    await invoke('start_sse_stream', { url: `${this.baseUrl}/event` })
    
    // Listen for events via Tauri's event system
    unlistenMessage = await listen('sse-message', (event) => {
        const data = JSON.parse(event.payload as string)
        onEvent(data)
    })
}
```

**SSE Event Types**:
- `session.updated` - Session metadata changed (title, timestamps)
- `message.part.updated` - Streaming message part (text, tools, files)
- `message.updated` - Complete message received
- `session.idle` - Session finished processing

---

## 2. HTTP Client Architecture

### Tauri Native HTTP (Rust Backend)
The app uses a custom HTTP client via Tauri's invoke system to bypass CORS restrictions:

**Rust Implementation** (`src-tauri/src/lib.rs`):
```rust
#[tauri::command]
async fn http_get(url: String) -> Result<HttpResponse, String> {
    let client = reqwest::Client::new();
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
    // ... parse response
    Ok(HttpResponse { status, data, ok })
}

#[tauri::command]
async fn http_post(url: String, body: serde_json::Value, headers: Option<HashMap<String, String>>) -> Result<HttpResponse, String>

#[tauri::command]
async fn http_patch(url: String, body: serde_json::Value, headers: Option<HashMap<String, String>>) -> Result<HttpResponse, String>

#[tauri::command]
async fn http_delete(url: String, headers: Option<HashMap<String, String>>) -> Result<HttpResponse, String>
```

**TypeScript Wrapper** (`src/services/http.ts`):
```typescript
export class TauriHttpClient {
    async get(url: string): Promise<Response> {
        const response = await invoke<HttpResponse>("http_get", { url })
        return new Response(JSON.stringify(response.data), { status: response.status })
    }
    // ... similar for post, patch, delete
    
    async fetch(input: string | URL | Request, options?: RequestInit): Promise<Response> {
        // Routes to appropriate method based on HTTP verb
    }
}

export const tauriFetch = (input, options) => tauriHttpClient.fetch(input, options)
```

### OpenCode SDK Integration
```typescript
// From src/services/opencode.ts
import { Opencode } from "@opencode-ai/sdk"

this.client = new Opencode({
    baseURL: absoluteBaseUrl,
    fetch: tauriFetch,  // Use Tauri's native HTTP client
})
```

---

## 3. SSE Implementation

### Rust SSE Stream Handler (`src-tauri/src/lib.rs`):
```rust
#[tauri::command]
async fn start_sse_stream(app: AppHandle, url: String) -> Result<(), String> {
    tokio::spawn(async move {
        match client.get(&url).send().await {
            Ok(response) => {
                let mut stream = response.bytes_stream();
                let mut buffer = String::new();
                
                while let Some(chunk) = stream.next().await {
                    // Buffer chunks until complete SSE message (\n\n)
                    while let Some(pos) = buffer.find("\n\n") {
                        let message = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();
                        
                        // Parse "data: {json}" lines
                        if let Some(data) = parse_sse_message(&message) {
                            let _ = app.emit("sse-message", data);
                        }
                    }
                }
            }
            Err(e) => {
                let _ = app.emit("sse-error", format!("Connection error: {}", e));
            }
        }
    });
    Ok(())
}

fn parse_sse_message(message: &str) -> Option<String> {
    for line in message.lines() {
        if line.starts_with("data: ") {
            return Some(line[6..].to_string());
        }
    }
    None
}
```

### Auto-Reconnection Logic (TypeScript):
```typescript
// Exponential backoff with jitter
const scheduleReconnect = () => {
    if (reconnectAttempts >= maxAttempts) return
    const delay = Math.min(8000, baseDelay * 2 ** reconnectAttempts) + jitter()
    reconnectAttempts++
    setTimeout(() => setupTauriSSE(), delay)
}
```

---

## 4. Server Management

### Server Types (`src/types/servers.ts`):
```typescript
export interface OpenCodeServer {
    id: string
    name: string
    protocol: "http" | "https"
    host: string
    port: number
    isDefault?: boolean
    lastConnected?: Date
    isDiscovered?: boolean    // Found via network scan
    discoveredAt?: Date
}
```

### Server Discovery (Network Scanning)
The Rust backend can scan local networks for opencode servers:

```rust
async fn discover_servers() -> Result<Vec<DiscoveredServer>, String> {
    // Scan common network ranges
    let local_ips = vec!["192.168.1", "192.168.0", "10.0.0", "172.16.0"]
    let ports = vec![4096, 3000, 8080, 8000, 3001, 8001]
    
    // Probe each IP:port for /app endpoint
    for i in 1..=20 {
        let url = format!("http://{}:{}/app", network_prefix, port)
        // 2 second timeout per probe
    }
}
```

### Server Service (`src/services/servers.ts`):
- Manages multiple server configurations
- Persists to localStorage
- Supports switching between servers
- Auto-discovery of LAN servers (currently disabled)
- Deduplicates discovered vs manual servers

---

## 5. Key Data Types

### Message Structure:
```typescript
interface OpenCodeMessage {
    id: string
    role: "user" | "assistant"
    content: string
    parts: OpenCodePart[]
    timestamp: Date
    providerID?: string
    modelID?: string
}

interface OpenCodePart {
    id: string
    type: "text" | "reasoning" | "file" | "tool" | "step-start" | "step-finish" | "snapshot" | "patch" | "agent"
    text?: string
    tool?: string
    filename?: string
    snapshot?: { id, title?, url?, data? }
    invocation?: { tool, input }
    state?: { status, error?, time?, input?, output? }
}
```

### Session Structure:
```typescript
interface OpenCodeSession {
    id: string
    title: string
    created: Date
    updated: Date
    parentID?: string      // For hierarchical sessions
    serverId?: string      // Which server owns this session
    serverName?: string
}
```

---

## 6. Key Files for Multi-User Adaptation

| File | Purpose | Relevance for Multi-User |
|------|---------|--------------------------|
| `src/services/opencode.ts` | Core API client | Would need to handle per-user/container authentication |
| `src/services/servers.ts` | Server management | Base for container-to-server mapping |
| `src/services/http.ts` | HTTP client wrapper | Add auth headers, container routing |
| `src-tauri/src/lib.rs` | Rust HTTP/SSE handlers | Add container lifecycle management |
| `src/types/servers.ts` | Server type definitions | Extend for container metadata |
| `src/App.tsx` | Main UI state management | User session isolation |

---

## 7. Architecture Patterns

### State Management
- React hooks + localStorage persistence
- No external state management library
- Services are singleton instances

### Error Handling
- Try/catch with console logging
- Retry logic for message sending (3 attempts with exponential backoff)
- Safety timeouts for loading states (30 second max)

### Message Flow
```
User Input -> sendMessage() -> SDK chat() -> Server
                                    |
                                    v
                              SSE Stream
                                    |
                                    v
           onEvent() callback -> Update React state -> UI re-render
```

---

## 8. Considerations for Multi-User Per-Container System

### What This Codebase Provides:
1. HTTP client architecture that can be extended with auth
2. SSE streaming infrastructure
3. Multi-server connection handling
4. Session management across servers

### What Would Need to Be Added:
1. **Container Management**: This app doesn't manage containers - would need integration with Podman API
2. **User Authentication**: No auth currently - "Don't expose ports publicly"
3. **Container-to-User Mapping**: Track which container serves which user
4. **Container Lifecycle**: Start/stop containers on user login/logout
5. **PTY/Terminal**: The opencode server handles terminal - client just sends/receives messages
6. **Resource Isolation**: Container limits, storage quotas

### Suggested Architecture for Multi-User:
```
                    +---------------------+
                    |   Frontend (React)   |
                    |   opencode2go UI     |
                    +----------+----------+
                               | HTTP/SSE
                    +----------v----------+
                    |   Gateway Server     |
                    |   - Auth/Session     |
                    |   - Container Mgmt   |
                    |   - Route to User    |
                    +----------+----------+
           +-------------------+-------------------+
           |                   |                   |
    +------v------+     +------v------+     +------v------+
    |  Container 1 |     |  Container 2 |     |  Container N |
    |  opencode    |     |  opencode    |     |  opencode    |
    |  User A      |     |  User B      |     |  User N      |
    +-------------+     +-------------+     +-------------+
```

---

## 9. Dependencies

**Frontend**:
- `@opencode-ai/sdk`: ^0.1.0-alpha.21 - Official opencode SDK
- `@tauri-apps/api`: ^2.6.0 - Tauri core APIs
- React 18, Vite, TypeScript

**Backend (Tauri/Rust)**:
- `reqwest` - HTTP client
- `tokio` - Async runtime
- `tokio-stream` - For SSE streaming
- `tauri` - Desktop framework

---

## 10. Security Considerations

From the README:
> This application does not include any authentication or security features.
> - Don't expose your opencode server ports publicly
> - Use secure networking solutions like VPN or Tailscale/Headscale
> - All communication is unencrypted HTTP by default

For a multi-user system, you would need:
- TLS/HTTPS for all communications
- JWT or session-based authentication
- Per-user container isolation
- Network policies between containers
- Audit logging

---

## 11. Terminal/PTY Handling

**Important**: opencode2go does NOT handle PTY directly. The terminal functionality is:
1. Managed entirely by the opencode server running inside the container
2. Commands are sent as messages via the chat API
3. Output is received as message parts via SSE
4. The `tool` part type with `state.output` contains command results

The opencode server exposes terminal capabilities through its message-based API, not a raw PTY stream. This simplifies the client but means:
- No interactive terminal session from the client
- All interactions are request/response via the chat API
- Tool executions (bash commands) are managed by the AI agent

---

## 12. Default Configuration

```typescript
// Default server
{
    host: "localhost",
    port: 4096,
    protocol: "http"
}

// Default appearance
{
    theme: "dracula",
    font: "JetBrains Mono",
    fontSize: 14
}

// Default permissions
{
    edit: "ask",
    bash: "ask"
}
```

---

## Summary

opencode2go is a well-structured client application that:
1. Connects to opencode servers via HTTP REST + SSE
2. Uses Tauri for cross-platform native capabilities
3. Manages multiple server connections
4. Provides a terminal-style UI for AI interactions

For adapting to a multi-user container system, the key insight is that **container management must be added as a separate layer** - either in a gateway service or by extending the Tauri backend. The existing HTTP/SSE infrastructure can be reused with authentication headers added.
