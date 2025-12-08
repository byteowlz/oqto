# Omni + ghostty-web Architecture Analysis

> Analysis Date: December 9, 2025
> Source: ~/byteowlz/omni
> Purpose: Understanding ghostty-web terminal integration and PTY handling for multi-user container system

## Overview

Omni is a Tauri-based "omnibar" application (like Spotlight/Alfred) that includes an embedded terminal using **ghostty-web**. This provides a full PTY-based terminal experience within a web/Tauri context.

**Key Insight**: Unlike opencode2go which uses a message-based terminal abstraction, Omni provides a **true PTY terminal** with raw input/output streams - exactly what's needed for a multi-user container system.

---

## 1. ghostty-web Integration

### Svelte Component (`src/lib/GhosttyTerminal.svelte`)

```svelte
<script lang="ts">
import { Terminal, FitAddon, init } from "ghostty-web";

export let font: string | undefined = undefined;
export let fontSize = 14;
export let theme: { background?: string; foreground?: string } | undefined;
export let focus = false;
export let allowInput = false;  // Read-only mode support

onMount(async () => {
    await init();  // Initialize WASM module
    
    term = new Terminal({
        fontFamily: font,
        fontSize,
        theme,
    });
    
    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    
    // Custom key handler for read-only mode
    term.attachCustomKeyEventHandler((ev) => {
        if (!allowInputFlag) {
            ev.preventDefault();
            ev.stopPropagation();
            return true;
        }
        return false;
    });
    
    term.open(container);
    fitAddon.fit();
    fitAddon.observeResize();
    
    // Event dispatchers for PTY communication
    dataDispose = term.onData((data) => dispatch("data", data));
    resizeDispose = term.onResize(({ cols, rows }) => 
        dispatch("resize", { cols, rows })
    );
});

// Exposed methods
export function write(data: string) { term?.write(data); }
export function focusTerminal() { term?.focus(); }
export function blurTerminal() { term?.blur(); }
export function getSize(): { cols: number; rows: number } | null
export function fit()
</script>
```

**Key Features**:
- Uses ghostty-web WASM module for terminal rendering
- FitAddon for automatic terminal sizing
- Custom key event handler for read-only mode
- Events: `data` (user input), `resize` (terminal size changes), `ready` (initialization complete)

---

## 2. PTY Management (Rust Backend)

### PTY Registry (`src-tauri/src/commands/pty.rs`)

```rust
use portable_pty::{native_pty_system, CommandBuilder, PtySize, MasterPty};

#[derive(Default)]
pub struct PtyRegistry {
    sessions: Mutex<HashMap<String, Arc<PtySession>>>,
}

pub struct PtySession {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
}
```

### Spawning a PTY Session

```rust
#[tauri::command]
pub fn spawn_pty(
    app: AppHandle,
    config: State<'_, Config>,
    cols: Option<u16>,
    rows: Option<u16>,
    cwd: Option<String>,
) -> AppResult<PtySpawnResult> {
    let pty_system = native_pty_system();
    let size = PtySize {
        rows: rows.unwrap_or(24),
        cols: cols.unwrap_or(80),
        pixel_width: 0,
        pixel_height: 0,
    };
    
    let pair = pty_system.openpty(size)?;
    
    let mut cmd = CommandBuilder::new(&config.shell);
    for arg in sanitize_shell_args_for_pty(&config.shell, &config.shell_args) {
        cmd.arg(arg);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    if let Some(dir) = cwd {
        cmd.cwd(dir);
    }
    
    let mut child = pair.slave.spawn_command(cmd)?;
    
    let mut reader = pair.master.try_clone_reader()?;
    let writer = master.take_writer()?;
    
    let session_id = Uuid::new_v4().to_string();
    
    // Reader thread - emits pty_data events
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app.emit("pty_data", PtyDataEvent {
                        id: session_id.clone(),
                        data: chunk,
                    });
                }
                Err(_) => break,
            }
        }
    });
    
    // Child watcher thread - emits pty_exit events
    std::thread::spawn(move || {
        let status = child.wait().ok().map(exit_to_code);
        let _ = app.emit("pty_exit", PtyExitEvent {
            id: session_id.clone(),
            status,
        });
    });
    
    Ok(PtySpawnResult { id: session_id })
}
```

### Writing to PTY

```rust
#[tauri::command]
pub fn write_pty(
    registry: State<'_, PtyRegistry>,
    id: String,
    data: String,
) -> AppResult<()> {
    let sessions = registry.sessions.lock().unwrap();
    if let Some(session) = sessions.get(&id) {
        let mut writer = session.writer.lock().unwrap();
        writer.write_all(data.as_bytes())?;
        writer.flush()?;
        Ok(())
    } else {
        Err(AppError::Io(format!("PTY session not found: {}", id)))
    }
}
```

### Resizing PTY

```rust
#[tauri::command]
pub fn resize_pty(
    registry: State<'_, PtyRegistry>,
    id: String,
    cols: u16,
    rows: u16,
) -> AppResult<()> {
    let sessions = registry.sessions.lock().unwrap();
    if let Some(session) = sessions.get(&id) {
        let master = session.master.lock().unwrap();
        master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })?;
        Ok(())
    } else {
        Err(AppError::Io(format!("PTY session not found: {}", id)))
    }
}
```

### Closing PTY

```rust
#[tauri::command]
pub fn close_pty(registry: State<'_, PtyRegistry>, id: String) -> AppResult<()> {
    let mut sessions = registry.sessions.lock().unwrap();
    if let Some(_session) = sessions.remove(&id) {
        // portable-pty drops will close the PTY
        Ok(())
    } else {
        Err(AppError::Io(format!("PTY session not found: {}", id)))
    }
}
```

---

## 3. Frontend-Backend Communication

### TypeScript Side (Main Page)

```typescript
// PTY event listeners
unlistenPtyData = await listen<{ id: string; data: string }>(
    "pty_data",
    (event) => handlePtyDataEvent(event.payload.id, event.payload.data)
);

unlistenPtyExit = await listen<{ id: string; status?: number | null }>(
    "pty_exit",
    (event) => handlePtyExitEvent(event.payload.id, event.payload.status)
);

// Handle PTY data - write to terminal
function handlePtyDataEvent(id: string, data: string) {
    if (!ptyId || id !== ptyId) return;
    terminalRef?.write(data);
}

// Handle PTY exit
function handlePtyExitEvent(id: string, status: number | null | undefined) {
    if (!ptyId || id !== ptyId) return;
    shellSessionStatus = status == null ? "Session ended" : `Session exited (${status})`;
    ptyId = null;
    terminalFocused = false;
}

// Terminal component event handlers
function handleTerminalData(event: CustomEvent<string>) {
    if (!ptyId) return;
    invoke("write_pty", { id: ptyId, data: event.detail });
}

function handleTerminalResize(event: CustomEvent<{ cols: number; rows: number }>) {
    if (!ptyId) return;
    const { cols, rows } = event.detail;
    invoke("resize_pty", { id: ptyId, cols, rows });
}

async function handleTerminalReady(event: CustomEvent<{ cols?: number; rows?: number }>) {
    terminalReady = true;
    await ensureShellSession(event.detail.cols, event.detail.rows);
}
```

### Session Management

```typescript
async function ensureShellSession(cols?: number, rows?: number) {
    if (ptyId) return ptyId;
    
    const res = await invoke("spawn_pty", {
        cols: cols ?? 80,
        rows: rows ?? 24,
        cwd: null,
    }) as { id: string };
    
    ptyId = res.id;
    return ptyId;
}

async function closeShellSession() {
    if (!ptyId) return;
    const id = ptyId;
    ptyId = null;
    await invoke("close_pty", { id });
}
```

---

## 4. Data Flow Diagram

```
+-------------------+     Events      +------------------+
|  GhosttyTerminal  | <-------------> |   Svelte Page    |
|  (ghostty-web)    |   on:data       |   (+page.svelte) |
|                   |   on:resize     |                  |
+-------------------+   on:ready      +--------+---------+
        ^                                      |
        |                                      | invoke()
        | write()                              v
        |                             +------------------+
        +-----------------------------+   Tauri Backend  |
              pty_data events         |   (Rust)         |
                                      +--------+---------+
                                               |
                                               | portable-pty
                                               v
                                      +------------------+
                                      |   PTY Session    |
                                      |   (Shell)        |
                                      +------------------+
```

---

## 5. Shell Mode Integration

The omnibar has a special "shell mode" triggered by `!` prefix:

```typescript
function maybeEnterShellMode() {
    if (shellMode) return;
    const trimmed = query.trimStart();
    if (trimmed.startsWith("!")) {
        shellMode = true;
        query = trimmed.slice(1).trimStart();
        ensureShellSession();
    }
}

async function runShellCommand() {
    const cmd = query.trim();
    if (!cmd) return;
    const id = await ensureShellSession();
    if (!id) return;
    
    // Write command with newline to execute
    await invoke("write_pty", { id, data: `${cmd}\n` });
    query = "";
}
```

---

## 6. Configuration

### Shell Configuration (`src-tauri/src/core/config.rs`)

```rust
pub struct Config {
    pub shell: String,           // e.g., "/bin/zsh"
    pub shell_args: Vec<String>, // e.g., ["-l"]
    pub terminal: TerminalConfig,
    // ...
}

pub struct TerminalConfig {
    pub font: Option<String>,
    pub font_size: Option<f64>,
    pub theme: Option<TerminalTheme>,
}

pub struct TerminalTheme {
    pub background: Option<String>,
    pub foreground: Option<String>,
}
```

### Shell Argument Sanitization

```rust
fn sanitize_shell_args_for_pty(shell: &str, args: &[String]) -> Vec<String> {
    let lower = shell.to_lowercase();
    
    // Avoid -c flags for interactive PTY
    let has_command_flag = args.iter().any(|a| {
        let al = a.to_lowercase();
        al == "-c" || al == "-lc" || al == "-command"
    });
    
    if !args.is_empty() && !has_command_flag {
        return args.to_vec();
    }
    
    // Safe defaults per shell
    if lower.contains("zsh") || lower.contains("bash") || lower.contains("fish") {
        return vec!["-l".to_string()];  // Login shell
    }
    if lower.contains("nu") {
        return vec![];  // Nushell loads config without -c
    }
    if lower.contains("powershell") || lower.contains("pwsh") {
        return vec!["-NoLogo".to_string()];
    }
    
    vec![]
}
```

---

## 7. Key Dependencies

### Rust (Cargo.toml implied)
- `portable-pty` - Cross-platform PTY abstraction
- `tauri` - Desktop framework
- `uuid` - Session ID generation
- `tokio` - Async runtime

### Frontend (package.json implied)
- `ghostty-web` - WebAssembly terminal emulator
- `svelte` - UI framework
- `@tauri-apps/api` - Tauri IPC

---

## 8. Adaptation for Multi-User Container System

### What This Provides

1. **True PTY sessions** with proper terminal emulation
2. **Session management** with unique IDs per PTY
3. **Bidirectional communication** (input/output/resize)
4. **Process lifecycle tracking** (spawn/exit events)
5. **Cross-platform shell support** (zsh, bash, fish, nushell, powershell)

### Key Patterns to Reuse

| Component | Pattern | Adaptation Need |
|-----------|---------|-----------------|
| `PtyRegistry` | HashMap of sessions | Add user/container mapping |
| `spawn_pty` | PTY creation | Connect to container instead of local shell |
| `pty_data` events | Output streaming | Route via WebSocket to correct user |
| `write_pty` | Input handling | Authenticate + route to correct container |
| `resize_pty` | Terminal resize | Pass through to container |
| `GhosttyTerminal` | UI component | Can reuse directly |

### Container Integration Strategy

Instead of spawning a local PTY with `portable-pty`, for containers:

```rust
// Option 1: Podman exec with PTY
pub async fn spawn_container_pty(
    container_id: &str,
    cols: u16,
    rows: u16,
) -> Result<ContainerPtySession> {
    // podman exec -it <container> /bin/bash
    // Use podman's attach API for PTY
}

// Option 2: SSH to container
pub async fn spawn_ssh_pty(
    container_ip: &str,
    user: &str,
    cols: u16,
    rows: u16,
) -> Result<SshPtySession> {
    // SSH with PTY allocation
    // Use async-ssh2 or similar
}

// Option 3: WebSocket relay
// Container runs a PTY server, we connect via WebSocket
pub async fn connect_container_websocket(
    container_url: &str,
    auth_token: &str,
) -> Result<WebSocketPtySession> {
    // Connect to ttyd or similar running in container
}
```

### Suggested Architecture

```
+-------------------+      WebSocket      +------------------+
|  Web Frontend     | <-----------------> |  Gateway Server  |
|  (ghostty-web)    |   per-user auth     |  (Rust/Go)       |
+-------------------+                     +--------+---------+
                                                   |
                                    +--------------+--------------+
                                    |              |              |
                               +----v----+   +----v----+   +----v----+
                               |Container|   |Container|   |Container|
                               | User A  |   | User B  |   | User C  |
                               | ttyd or |   | ttyd or |   | ttyd or |
                               | sshd    |   | sshd    |   | sshd    |
                               +---------+   +---------+   +---------+
```

### Gateway Server Responsibilities

1. **Authentication**: JWT/session validation
2. **Container routing**: Map user -> container
3. **PTY relay**: Forward input/output between client and container
4. **Session management**: Track active terminals per user
5. **Container lifecycle**: Start containers on demand, cleanup on disconnect

---

## 9. ghostty-web Specifics

### Initialization

```typescript
import { Terminal, FitAddon, init } from "ghostty-web";

// Must call init() before creating Terminal
await init();  // Loads WASM module

const term = new Terminal({
    fontFamily: "JetBrains Mono",
    fontSize: 14,
    theme: {
        background: "#1a1b26",
        foreground: "#a9b1d6"
    }
});
```

### Terminal API

```typescript
// Core methods
term.open(container: HTMLElement)
term.write(data: string)
term.focus()
term.blur()
term.dispose()

// Addons
term.loadAddon(addon)

// Events
term.onData((data: string) => void)       // User typed input
term.onResize(({ cols, rows }) => void)   // Terminal resized

// Custom key handling
term.attachCustomKeyEventHandler((ev: KeyboardEvent) => boolean)
```

### FitAddon

```typescript
const fitAddon = new FitAddon();
term.loadAddon(fitAddon);

fitAddon.fit();           // Resize terminal to container
fitAddon.observeResize(); // Auto-fit on container resize
fitAddon.dispose();
```

---

## 10. Security Considerations

### Current Omni Security
- Local-only PTY (no network exposure)
- Single-user desktop app
- No authentication needed

### Multi-User Container Security Needs
1. **Authentication**: User login before terminal access
2. **Authorization**: Users only access their own containers
3. **Isolation**: Container network separation
4. **Encryption**: TLS for all WebSocket connections
5. **Rate limiting**: Prevent DoS via PTY spam
6. **Audit logging**: Track all terminal sessions
7. **Session timeout**: Auto-disconnect idle sessions

---

## Summary

Omni's ghostty-web integration provides a production-ready reference for:

1. **Terminal rendering**: ghostty-web handles all xterm emulation
2. **PTY management**: portable-pty provides cross-platform PTY creation
3. **Event-based communication**: Clean separation via Tauri events
4. **Session lifecycle**: Proper spawn/cleanup with unique session IDs

For the multi-user container system, the main adaptation is replacing `portable-pty` local spawning with container-based PTY connections (via podman exec, SSH, or WebSocket relay to ttyd).
