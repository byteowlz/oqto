# Pi-Daemon Design Document

## Executive Summary

A pi-daemon would run pi as a long-running background process that maintains loaded resources (extensions, skills, prompts, themes, models) and serves multiple client sessions. This reduces startup time from 300-800ms to <50ms for subsequent invocations.

## Current Architecture vs. Daemon Architecture

### Current (per-invocation)

```
User runs: pi --mode rpc
  ↓
Parse args
  ↓
Load resources (extensions, skills, prompts, themes) ← SLOW
  ↓
Initialize ModelRegistry
  ↓
Create AgentSession
  ↓
Accept commands on stdin
  ↓
Send events on stdout
  ↓
Process exits when stdin closes
```

### Daemon Architecture

```
pi-daemon (long-running process)
  ├─ Load resources ONCE at startup
  ├─ Maintain shared cache
  ├─ Watch for file changes (hot reload)
  ├─ Accept client connections via Unix socket
  └─ Spawn sessions per client
        ↓
Client: pi --daemon-client
  ↓
Connect to daemon (or start if not running)
  ↓
Request session with cwd/options
  ↓
Receive session ID
  ↓
Send commands, receive events (over socket)
  ↓
Disconnect (daemon keeps session alive for reconnect)
```

## Core Components Required

### 1. Daemon Process (`pi-daemon`)

#### Process Management

```typescript
// packages/coding-agent/src/daemon/daemon-process.ts
interface DaemonConfig {
  socketPath: string;           // e.g., /tmp/pi-daemon-<uid>.sock
  pidFile: string;              // e.g., /tmp/pi-daemon.pid
  logFile: string;              // e.g., ~/.local/share/pi/daemon.log
  idleTimeout: number;          // Auto-shutdown after N seconds idle
  maxSessions: number;          // Maximum concurrent sessions
  maxMemory: number;            // Max memory before forcing GC
}

class PiDaemon {
  private server: net.Server;
  private sessions = new Map<string, DaemonSession>();
  private resourceCache: ResourceCache;
  private idleTimer: ReturnType<typeof setTimeout> | null = null;
  private lastActivity: number;

  async start(): Promise<void> {
    // 1. Create Unix socket
    this.server = net.createServer(this.handleConnection.bind(this));
    await new Promise<void>((resolve) => {
      this.server.listen(this.config.socketPath, resolve);
    });

    // 2. Load resources once
    this.resourceCache = await ResourceLoader.loadOnce();

    // 3. Start file watchers for hot reload
    this.watchResources();

    // 4. Set up signal handlers
    this.setupSignalHandlers();

    // 5. Write PID file
    writeFileSync(this.config.pidFile, String(process.pid));

    // 6. Log ready status
    console.log(`pi-daemon started on ${this.config.socketPath}`);
  }

  private handleConnection(socket: net.Socket): void {
    const clientId = crypto.randomUUID();
    const connection = new DaemonConnection(socket, clientId);

    connection.on('request', async (request: DaemonRequest) => {
      this.resetIdleTimer();

      switch (request.type) {
        case 'create_session':
          await this.handleCreateSession(connection, request);
          break;
        case 'attach_session':
          await this.handleAttachSession(connection, request);
          break;
        case 'list_sessions':
          await this.handleListSessions(connection);
          break;
        case 'ping':
          connection.send({ type: 'pong' });
          break;
        case 'shutdown':
          await this.handleShutdown(connection);
          break;
      }
    });

    connection.on('close', () => {
      // Session stays alive for reconnect
    });
  }
}
```

#### Resource Cache

```typescript
// packages/coding-agent/src/daemon/resource-cache.ts
interface ResourceCache {
  version: string;           // Cache format version
  extensions: Extension[];
  skills: Skill[];
  prompts: PromptTemplate[];
  themes: Theme[];
  models: Model[];
  settings: Settings;
  loadTime: number;
  checksums: Map<string, string>;  // File checksums for validation
}

class ResourceCache {
  static async loadOnce(cwd: string, agentDir: string): Promise<ResourceCache> {
    const startTime = Date.now();

    // Load everything in parallel where possible
    const [
      settings,
      authStorage,
      modelRegistry,
      resourceLoader,
    ] = await Promise.all([
      SettingsManager.create(cwd, agentDir),
      AuthStorage.load(),
      ModelRegistry.load(agentDir),
      ResourceLoader.loadAll(),
    ]);

    const cache: ResourceCache = {
      version: '1',
      extensions: resourceLoader.getExtensions().extensions,
      skills: resourceLoader.getSkills().skills,
      prompts: resourceLoader.getPrompts().prompts,
      themes: resourceLoader.getThemes().themes,
      models: modelRegistry.getAll(),
      settings: settings.getAll(),
      loadTime: Date.now() - startTime,
      checksums: await this.computeChecksums(agentDir),
    };

    return cache;
  }

  static async computeChecksums(baseDir: string): Promise<Map<string, string>> {
    const checksums = new Map<string, string>();

    // Compute checksums for all resource files
    for await (const file of this.walkResources(baseDir)) {
      checksums.set(file, await this.hashFile(file));
    }

    return checksums;
  }

  private static async hashFile(path: string): Promise<string> {
    const content = await fs.readFile(path);
    return crypto.createHash('sha256').update(content).digest('hex');
  }

  async reloadIfChanged(): Promise<boolean> {
    const currentChecksums = await ResourceCache.computeChecksums(this.baseDir);

    for (const [path, checksum] of this.checksums.entries()) {
      if (currentChecksums.get(path) !== checksum) {
        // File changed - reload cache
        await this.reload();
        return true;
      }
    }

    return false;
  }
}
```

#### Session Management

```typescript
// packages/coding-agent/src/daemon/session-manager.ts
interface DaemonSession {
  id: string;
  cwd: string;
  agentSession: AgentSession;
  connections: Set<DaemonConnection>;
  createdAt: number;
  lastActivity: number;
}

class DaemonSession {
  constructor(
    public id: string,
    public cwd: string,
    private resourceCache: ResourceCache,
    options: CreateSessionOptions,
  ) {
    // Create AgentSession using cached resources
    this.agentSession = new AgentSession({
      ...options,
      resourceLoader: this.createResourceLoaderFromCache(),
      modelRegistry: this.createModelRegistryFromCache(),
    });

    this.createdAt = Date.now();
    this.lastActivity = Date.now();
  }

  attach(connection: DaemonConnection): void {
    this.connections.add(connection);
    this.lastActivity = Date.now();

    // Forward session events to connection
    this.agentSession.subscribe((event) => {
      connection.send({ type: 'event', sessionId: this.id, event });
    });

    // Remove connection on disconnect
    connection.on('close', () => {
      this.connections.delete(connection);
    });
  }

  async executeCommand(command: RpcCommand): Promise<RpcResponse> {
    this.lastActivity = Date.now();
    // Delegate to existing AgentSession logic
    return handleRpcCommand(this.agentSession, command);
  }

  async destroy(): Promise<void> {
    // Close all connections
    for (const conn of this.connections) {
      conn.close();
    }

    // Cleanup session
    await this.agentSession.destroy();
  }
}
```

### 2. Client (`pi --daemon-client`)

#### Client Connection

```typescript
// packages/coding-agent/src/daemon/client.ts
class DaemonClient {
  private socket: net.Socket | null = null;

  async connect(): Promise<void> {
    const socketPath = getSocketPath();

    // Try to connect to existing daemon
    try {
      this.socket = net.createConnection({ path: socketPath });
      await new Promise<void>((resolve, reject) => {
        this.socket!.once('connect', resolve);
        this.socket!.once('error', reject);
      });
      return;
    } catch (err) {
      // Daemon not running - start it
      console.log('Starting pi-daemon...');
      await spawnDaemon();
      await this.waitForDaemon();
      this.socket = net.createConnection({ path: socketPath });
    }
  }

  async createSession(cwd: string, options: CreateSessionOptions): Promise<string> {
    const request: DaemonRequest = {
      type: 'create_session',
      cwd,
      options,
    };

    this.send(request);
    const response = await this.waitForResponse('session_created');
    return response.sessionId;
  }

  async attachSession(sessionId: string): Promise<void> {
    const request: DaemonRequest = {
      type: 'attach_session',
      sessionId,
    };

    this.send(request);
    await this.waitForResponse('session_attached');
  }

  async executeCommand(sessionId: string, command: RpcCommand): Promise<RpcResponse> {
    const request: DaemonRequest = {
      type: 'execute_command',
      sessionId,
      command,
    };

    this.send(request);
    return this.waitForResponse('command_response');
  }

  onEvent(callback: (event: any) => void): void {
    this.socket!.on('data', (data) => {
      const message = JSON.parse(data.toString());
      if (message.type === 'event') {
        callback(message.event);
      }
    });
  }
}
```

### 3. Protocol Design

#### Daemon Protocol (over Unix socket)

```typescript
// packages/coding-agent/src/daemon/protocol.ts

// Client → Daemon requests
type DaemonRequest =
  | { type: 'create_session'; cwd: string; options: CreateSessionOptions }
  | { type: 'attach_session'; sessionId: string }
  | { type: 'execute_command'; sessionId: string; command: RpcCommand }
  | { type: 'list_sessions' }
  | { type: 'ping' }
  | { type: 'shutdown' }
  | { type: 'get_stats' };

// Daemon → Client responses
type DaemonResponse =
  | { type: 'session_created'; sessionId: string }
  | { type: 'session_attached' }
  | { type: 'command_response'; response: RpcResponse }
  | { type: 'sessions_list'; sessions: SessionInfo[] }
  | { type: 'pong' }
  | { type: 'stats'; stats: DaemonStats }
  | { type: 'error'; error: string };

// Daemon → Client events (streamed)
type DaemonEvent =
  | { type: 'event'; sessionId: string; event: AgentSessionEvent }
  | { type: 'resource_reloaded' }
  | { type: 'session_destroyed'; sessionId: string };

interface SessionInfo {
  id: string;
  cwd: string;
  createdAt: number;
  lastActivity: number;
  hasConnections: boolean;
}

interface DaemonStats {
  uptime: number;
  sessionCount: number;
  connectionCount: number;
  memoryUsage: NodeJS.MemoryUsage;
  resourceCacheSize: number;
}
```

### 4. File System Integration

#### Directory Watching

```typescript
// packages/coding-agent/src/daemon/file-watcher.ts
import chokidar from 'chokidar';

class ResourceWatcher {
  private watcher: chokidar.FSWatcher;

  constructor(
    private resourceCache: ResourceCache,
    private onReload: () => Promise<void>,
  ) {
    // Watch all resource directories
    const watchPaths = [
      join(agentDir, 'extensions'),
      join(agentDir, 'skills'),
      join(agentDir, 'prompts'),
      join(agentDir, 'themes'),
      join(cwd, '.pi', 'extensions'),
      join(cwd, '.pi', 'skills'),
      join(cwd, '.pi', 'prompts'),
      join(cwd, '.pi', 'themes'),
      modelsJsonPath,
      settingsPath,
    ];

    this.watcher = chokidar.watch(watchPaths, {
      ignoreInitial: true,
      awaitWriteFinish: { stabilityThreshold: 200, pollInterval: 100 },
    });

    // Debounce reloads to avoid thrashing
    this.watcher.on('all', debounce(this.handleFileChange.bind(this), 500));
  }

  private async handleFileChange(event: string, path: string): Promise<void> {
    console.log(`Resource changed: ${path}`);

    // Check if cache needs reload
    const needsReload = await this.resourceCache.reloadIfChanged();

    if (needsReload) {
      await this.onReload();

      // Notify all connected clients
      for (const conn of this.connections) {
        conn.send({ type: 'resource_reloaded' });
      }
    }
  }

  stop(): void {
    this.watcher.close();
  }
}
```

### 5. CLI Integration

#### Main Entry Point Changes

```typescript
// packages/coding-agent/src/main.ts

// Add daemon mode
if (parsed.daemon) {
  const daemon = new PiDaemon({
    socketPath: getDaemonSocketPath(),
    pidFile: getDaemonPidPath(),
    logFile: getDaemonLogPath(),
    idleTimeout: 3600,  // 1 hour
    maxSessions: 50,
    maxMemory: 1024 * 1024 * 1024,  // 1GB
  });

  await daemon.start();
  return;
}

// Add daemon client mode (default for RPC)
if (parsed.mode === 'rpc') {
  const client = new DaemonClient();
  await client.connect();

  const sessionId = await client.createSession(cwd, sessionOptions);
  await client.attachSession(sessionId);

  // Forward stdin commands to daemon
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  rl.on('line', async (line) => {
    const command = JSON.parse(line);
    const response = await client.executeCommand(sessionId, command);
    console.log(JSON.stringify(response));
  });

  client.onEvent((event) => {
    console.log(JSON.stringify(event));
  });

  return;
}
```

### 6. Configuration

#### Daemon Settings

```typescript
// ~/.local/share/pi/daemon.json
interface DaemonSettings {
  enabled: boolean;           // Enable daemon mode
  socketPath?: string;         // Custom socket path
  idleTimeout?: number;        // Seconds before auto-shutdown
  maxSessions?: number;        // Max concurrent sessions
  maxMemory?: number;          // Max memory in bytes
  logLevel?: 'debug' | 'info' | 'warn' | 'error';
  hotReload?: boolean;         // Watch files and reload
  persistent?: boolean;        // Start on system boot?
}

const defaultDaemonSettings: DaemonSettings = {
  enabled: true,
  idleTimeout: 3600,  // 1 hour
  maxSessions: 50,
  maxMemory: 1024 * 1024 * 1024,  // 1GB
  logLevel: 'info',
  hotReload: true,
  persistent: false,
};
```

### 7. Security Considerations

#### Socket Permissions

```typescript
function createSocket(): net.Server {
  const socketPath = getDaemonSocketPath();
  const uid = process.getuid();

  // Remove existing socket with permissions check
  if (fs.existsSync(socketPath)) {
    const stats = fs.statSync(socketPath);
    // Only delete if we own it
    if (stats.uid === uid) {
      fs.unlinkSync(socketPath);
    } else {
      throw new Error(`Socket exists but is not owned by user ${uid}`);
    }
  }

  // Create with restricted permissions (owner only)
  const server = net.createServer();

  server.on('listening', () => {
    fs.chmodSync(socketPath, 0o600);  // rw-------
  });

  return server;
}
```

#### Authentication

```typescript
class DaemonConnection {
  private authenticated = false;
  private authToken: string;

  constructor(socket: net.Socket, private clientId: string) {
    this.authToken = this.generateToken();

    socket.on('data', (data) => {
      if (!this.authenticated) {
        this.handleAuth(data);
      } else {
        this.handleMessage(data);
      }
    });
  }

  private handleAuth(data: Buffer): void {
    try {
      const message = JSON.parse(data.toString());

      // Simple token-based auth (same user ID)
      const expectedToken = this.getExpectedToken();
      if (message.token === expectedToken) {
        this.authenticated = true;
        this.send({ type: 'auth_success' });
      } else {
        this.send({ type: 'auth_failed' });
        this.socket.destroy();
      }
    } catch (err) {
      this.socket.destroy();
    }
  }

  private generateToken(): string {
    // Token is based on user ID and a secret
    const uid = process.getuid();
    const secret = fs.readFileSync('/etc/machine-id', 'utf-8').trim();
    return crypto.createHash('sha256')
      .update(`${uid}:${secret}`)
      .digest('hex')
      .slice(0, 32);
  }
}
```

### 8. Lifecycle Management

#### Start/Stop/Status Commands

```bash
# Start daemon manually
pi daemon start

# Stop daemon
pi daemon stop

# Restart daemon
pi daemon restart

# Check status
pi daemon status

# Get daemon stats
pi daemon stats

# Show active sessions
pi daemon list-sessions

# Kill specific session
pi daemon kill-session <session-id>
```

#### Status Output

```typescript
interface DaemonStatus {
  status: 'running' | 'stopped' | 'unknown';
  pid?: number;
  uptime?: number;
  sessions?: SessionInfo[];
  memory?: NodeJS.MemoryUsage;
}
```

### 9. Monitoring and Debugging

#### Health Checks

```typescript
class PiDaemon {
  private healthCheckInterval: NodeJS.Timeout;

  startHealthCheck(): void {
    this.healthCheckInterval = setInterval(() => {
      this.performHealthCheck();
    }, 30000);  // Every 30 seconds
  }

  private performHealthCheck(): void {
    const memUsage = process.memoryUsage();

    // Check memory usage
    if (memUsage.heapUsed > this.config.maxMemory) {
      console.warn('Memory usage high, forcing GC');
      if (global.gc) {
        global.gc();
      }
    }

    // Check for orphaned sessions (no connections for > 1 hour)
    for (const [id, session] of this.sessions) {
      const idleTime = Date.now() - session.lastActivity;
      if (idleTime > 3600000 && session.connections.size === 0) {
        console.log(`Destroying idle session: ${id}`);
        session.destroy();
        this.sessions.delete(id);
      }
    }

    // Log stats
    console.log(JSON.stringify({
      timestamp: Date.now(),
      sessions: this.sessions.size,
      memory: memUsage,
      uptime: process.uptime(),
    }));
  }
}
```

#### Log Management

```typescript
class DaemonLogger {
  private logStream: fs.WriteStream;

  constructor(logFile: string) {
    this.logStream = fs.createWriteStream(logFile, { flags: 'a' });

    // Rotate logs daily
    this.setupLogRotation();
  }

  log(level: string, message: string, meta?: any): void {
    const entry = {
      timestamp: new Date().toISOString(),
      level,
      message,
      ...meta,
    };

    this.logStream.write(JSON.stringify(entry) + '\n');
  }

  private setupLogRotation(): void {
    // Rotate at midnight
    const now = new Date();
    const midnight = new Date(
      now.getFullYear(),
      now.getMonth(),
      now.getDate() + 1,
      0, 0, 0
    );

    const msUntilMidnight = midnight.getTime() - now.getTime();
    setTimeout(() => {
      this.rotateLog();
      setInterval(() => this.rotateLog(), 24 * 60 * 60 * 1000);
    }, msUntilMidnight);
  }
}
```

## Implementation Roadmap

### Phase 1: Core Daemon (2 weeks)
- [ ] Daemon process skeleton
- [ ] Unix socket server
- [ ] Session manager
- [ ] Client connection handling
- [ ] Basic protocol (create_session, execute_command)
- [ ] PID file management
- [ ] Signal handlers (SIGTERM, SIGINT)

### Phase 2: Client Integration (1 week)
- [ ] Daemon client class
- [ ] Auto-start daemon on connection failure
- [ ] CLI integration (--daemon, --daemon-client)
- [ ] Fallback to direct invocation if daemon fails
- [ ] Connection error handling

### Phase 3: Resource Caching (1 week)
- [ ] ResourceCache class
- [ ] Load resources once at daemon startup
- [ ] Share cache across sessions
- [ ] File checksum computation
- [ ] Cache invalidation logic

### Phase 4: Hot Reload (1 week)
- [ ] File watcher with chokidar
- [ ] Debounce file changes
- [ ] Reload cache on change
- [ ] Notify clients of reload
- [ ] Graceful handling of reload errors

### Phase 5: Lifecycle Management (1 week)
- [ ] Idle timeout with auto-shutdown
- [ ] Start/stop/status commands
- [ ] Session listing
- [ ] Memory management and GC
- [ ] Health checks

### Phase 6: Security (3 days)
- [ ] Socket permissions
- [ ] Token-based authentication
- [ ] User isolation
- [ ] Audit logging

### Phase 7: Monitoring (3 days)
- [ ] Stats endpoint
- [ ] Log file management
- [ ] Log rotation
- [ ] Debug mode

### Phase 8: Testing (1 week)
- [ ] Unit tests for daemon
- [ ] Integration tests for client
- [ ] Concurrent session tests
- [ ] Hot reload tests
- [ ] Memory leak tests

### Phase 9: Documentation (2 days)
- [ ] Daemon architecture docs
- [ ] Configuration guide
- [ ] Troubleshooting guide
- [ ] Migration guide

## Testing Strategy

### Unit Tests
```typescript
// test/daemon/daemon.test.ts
describe('PiDaemon', () => {
  it('should start and accept connections');
  it('should create sessions');
  it('should route commands to correct session');
  it('should clean up orphaned sessions');
  it('should shutdown gracefully');
});
```

### Integration Tests
```typescript
// test/daemon/integration.test.ts
describe('Daemon Integration', () => {
  it('should auto-start daemon when connecting');
  it('should persist session across reconnects');
  it('should reload resources when files change');
  it('should handle concurrent clients');
  it('should enforce max session limit');
});
```

### Performance Tests
```typescript
// test/daemon/performance.test.ts
describe('Daemon Performance', () => {
  it('should startup in < 100ms with cache');
  it('should handle 1000 commands/second per session');
  it('should support 50 concurrent sessions');
  it('should not leak memory over time');
});
```

## Risk Assessment

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Daemon crashes, losing sessions | High | Medium | Session persistence, watchdog restart |
| Memory leaks in long-running process | High | Medium | Health checks, memory limits, forced GC |
| Socket permissions issues | Medium | Low | Permission validation, fallback to direct mode |
| Resource cache staleness | Medium | Low | File watchers, checksum validation |
| Concurrent session contention | Medium | Low | Session isolation, resource locking |
| Security vulnerabilities | High | Low | Authentication, user isolation, audit logs |

## Alternatives Considered

### Alternative 1: Worker Thread
- **Pros:** Simpler IPC (shared memory), faster communication
- **Cons:** Process isolation lost, single crash affects all
- **Verdict:** Good for single-user, but process isolation is safer

### Alternative 2: HTTP Server
- **Pros:** Language agnostic clients, familiar protocol
- **Cons:** Slower than Unix sockets, more complex authentication
- **Verdict:** Unix sockets are faster and simpler for local use

### Alternative 3: D-Bus
- **Pros:** Standard Linux IPC, built-in security
- **Cons:** Not cross-platform, complex API
- **Verdict:** Too platform-specific

## Success Metrics

- **Startup time:** < 50ms (from 300-800ms)
- **Memory overhead:** < 200MB (including resource cache)
- **Command latency:** < 5ms (vs. current ~1ms over stdin/stdout)
- **Max concurrent sessions:** 50+ without degradation
- **Hot reload time:** < 500ms
- **Daemon crash rate:** < 0.1% of invocations
- **Client fallback success:** 100% (if daemon unavailable)

## Open Questions

1. **Session persistence:** Should sessions be written to disk for recovery after daemon crash?
2. **Multi-user:** Should daemon support multiple users with separate caches?
3. **Network sockets:** Should we allow TCP sockets for remote access?
4. **Session sharing:** Should multiple clients be able to attach to the same session?
5. **Graceful shutdown:** How to handle in-flight LLM requests during shutdown?

## Dependencies

```json
{
  "chokidar": "^3.5.3",      // File watching
  "socket.io": "^4.7.2",    // Optional: for richer protocol
  "node-cron": "^3.0.3",    // For scheduled tasks
  "pino": "^8.16.2"         // Structured logging
}
```

## Conclusion

A pi-daemon would provide significant performance benefits for Oqto's Main Chat (which uses RPC mode) and any other use cases involving frequent pi invocations. The implementation is straightforward but requires careful attention to:

1. Process lifecycle management
2. Session isolation and cleanup
3. Resource cache invalidation
4. Security (socket permissions, authentication)
5. Monitoring and debugging capabilities

With a 10-week implementation timeline, this is a realistic and high-impact project.
