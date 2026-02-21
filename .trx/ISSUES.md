# Issues

## Open

### [oqto-n7zc] setup.sh: fix full user provisioning pipeline (pi, eavs, models, runner) (P0, epic)
setup.sh must correctly provision everything for a new platform user on a fresh install:

1. Pi installation: install pi-coding-agent system-wide with a bun wrapper that works for all users (not hardcoded to installer's HOME)
2. Eavs config: always generate proper config under oqto user with [keys] enabled, master key, and env file
3. Per-user provisioning on login/creation:
...


### [oqto-c6n3] Refactor: route all mux file operations through oqto-files instead of RunnerUserPlane (P1, task)

### [oqto-fr2f] Admin UI: manage eavs providers and sync models.json (P1, feature)
Add admin interface for managing LLM providers:

1. Admin settings page shows configured eavs providers with status (API key set/missing, test result)
2. Admins can add/remove/edit providers (type, API key, base_url)
3. Provider API keys are written to eavs env file, provider config to eavs config.toml
...


### [oqto-q4ae] setup.sh must guarantee hstry is running and accessible for every provisioned user (P1, bug)
Production shows 500 on /api/chat-history with 'Chat history service not configured for this user.' for wismut-vCmT. Root cause: get_runner_for_user() returns None in multi-user mode, which is a hard error. The setup process needs to:

1. Verify hstry binary is installed and accessible
2. Verify per-user hstry config exists (~/.config/hstry/config.toml)
3. Verify per-user systemd service is created, enabled, and started
...


### [oqto-mvdv] Streaming reliability: backpressure handling, reconnect resync, and delta coalescing (P1, epic)
Our WebSocket layer (ws-manager.ts) assumes a perfect connection. Pi-mobile (https://github.com/ayagmar/pi-mobile) demonstrates several transport reliability patterns we lack that cause real user-facing issues: silent message loss under backpressure, corrupted UI after reconnect, excessive re-renders during fast streaming, and no cross-device drift detection.

This epic addresses the foundational transport reliability gap between Octo and pi-mobile's approach. The core philosophy: assume the connection will break, and design every layer around recovery.

## Current Problems
...


### [octo-nqg8.9] Replace unsafe env::set_var in octo-runner and test code (P1, task)
Two categories of unsafe env::set_var/remove_var:

1. bin/octo-runner.rs line 3650 - set_var at startup for .env loading. Replace with a proper env file loading approach: either collect all vars into a HashMap and apply them before spawning threads, or use a crate like dotenvy which handles this safely.

2. auth/config.rs tests (4 blocks) and local/sandbox.rs tests (3 blocks) - set_var/remove_var in tests. Replace with the temp_env crate (temp_env::with_var/with_vars) which provides scoped env var overrides that are automatically cleaned up, or use serial_test with proper isolation.

### [octo-nqg8.8] Replace unsafe termios code in octoctl with rpassword crate (P1, task)
ctl/main.rs uses 4 unsafe blocks (lines 2274-2291) for terminal echo suppression during password input via raw libc::tcgetattr/tcsetattr. Replace entire read_password() function with the rpassword crate which provides a safe, cross-platform rpassword::read_password() function that handles all the terminal manipulation safely.

### [octo-nqg8.7] Replace unsafe libc calls with nix crate safe wrappers (P1, task)
Replace all unsafe libc calls with safe alternatives from the nix crate (already listed in workspace Cargo.toml):

1. api/handlers/projects.rs - libc::geteuid()/getegid() (4 blocks) -> nix::unistd::geteuid()/getegid()
2. local/sandbox.rs - libc::getpwnam + pointer deref (2 blocks) -> nix::unistd::User::from_name()
3. runner/client.rs - libc::getpwnam + pointer deref (2 blocks) -> nix::unistd::User::from_name()
...


### [octo-nqg8.6] Remove dead code in user_plane/, ws/, canon/, container/, api/, session/, templates/, onboarding/, workspace/, hstry/ (P1, task)
Remove remaining dead code across smaller modules:
- user_plane/types.rs (7): SessionInfo, StartSessionRequest, StartSessionResponse, MainChatSessionInfo, MainChatMessage, MemoryEntry, MemorySearchResults
- user_plane/mod.rs: unused UserPlane trait methods
- user_plane/runner.rs: RunnerUserPlane::for_user
- ws/types.rs (11): WsEvent variants (SessionUpdated, SessionError, AgentReconnecting, ThinkingDelta, MessageUpdated, ToolStart, ToolEnd, PermissionResolved, QuestionResolved, A2uiActionResolved, OpencodeEvent), unused WsCommand fields, WsCommand::session_id method, Ping variant
...


### [octo-nqg8.5] Remove dead code in local/ module (process.rs, runtime.rs, sandbox.rs, linux_users.rs, user_hstry.rs) (P1, task)
Remove ~17 dead code items:
- local/process.rs (5): RunAsUser struct, ProcessHandle::new, ProcessManager methods (spawn_fileserver, spawn_ttyd, spawn_as_user, spawn_sandboxed, stop_session), shell_escape fn
- local/runtime.rs (2): LocalRuntime::start_session, LocalRuntime::stop_session
- local/sandbox.rs (9): SandboxConfig methods (minimal, strict, from_profile, from_profile_with_custom, load_from_workspace, merge_with_workspace, with_workspace_config, build_bwrap_args_for_user, is_bwrap_available)
- local/linux_users.rs (6): PROJECT_PREFIX const, LinuxUsersConfig methods (project_username, ensure_project_user, chown_directory_to_user, effective_username, ensure_effective_user)
...


### [octo-nqg8.4] Remove dead code in runner/pi_translator.rs and runner/client.rs (P1, task)
Remove dead code:
- runner/pi_translator.rs (8+): PiTranslator struct and all methods (new, set_pending_client_id, translate, on_agent_start, on_agent_end, etc), plus standalone functions (pi_response_to_canonical, pi_agent_message_to_canonical, pi_content_to_parts, pi_image_to_part, pi_stop_reason, extract_text_content)
- runner/client.rs (8+): write_stdin, subscribe_stdout, list_sessions, get_session, list_main_chat_sessions, search_memories, add_memory, delete_memory, pi_unsubscribe, pi_get_last_assistant_text, pi_set_steering_mode, pi_set_follow_up_mode, pi_export_html, pi_bash, pi_abort_bash, StdoutSubscription, StdoutSubscriptionEvent, PiSubscription and its methods

### [octo-nqg8.3] Remove dead code in history/ module (repository.rs, service.rs, models.rs) (P1, task)
Remove ~19 dead code items:
- history/repository.rs (14): hstry_db_path, HSTRY_POOL_CACHE static, open_hstry_pool, hstry_timestamp_ms, list_sessions_from_hstry, get_session_from_hstry, get_session_messages_from_hstry, list_sessions, list_sessions_from_dir, list_sessions_grouped, get_session, get_session_from_dir, update_session_title, update_session_title_in_dir
- history/service.rs (3): get_session_messages_async, get_session_messages_rendered_from_hstry, get_session_messages_rendered
- history/models.rs (2): SessionInfo struct, SessionTime struct
These are legacy history functions superseded by hstry gRPC.

### [octo-nqg8.2] Remove dead code in pi/ module (runtime.rs, types.rs, session_parser.rs, client.rs) (P1, task)
Remove ~31 dead code items across the pi module:
- pi/runtime.rs (15): PiSpawnConfig, PiProcess trait, PiRuntime trait, LocalPiRuntime, LocalPiProcess, RunnerPiRuntime, RunnerPiProcess, ContainerPiRuntime, ContainerPiProcess and all their methods
- pi/types.rs (13): PiCommand enum, ImageContent, ImageSource, PiResponse, PiEvent enum, AssistantMessageEvent, ToolResultMessage, ContentBlock, ToolCall, ToolResult, ExtensionUiRequest, PiMessage, PiMessage::parse
- pi/session_parser.rs (3): TITLE_PATTERN static, ParsedTitle struct and all methods
- pi/client.rs (1): PiClientConfig struct
...


### [octo-nqg8.1] Remove dead code in runner/pi_manager.rs (P1, task)
Remove ~28 dead code items: PiSessionManager, PiSession, PiManagerConfig, PiSessionConfig, PiSessionCommand, PiEventWrapper, PendingResponses, PendingClientId, HstryExternalId, ModelCacheEntry, JsonlMessageKey, JsonlEntry, JsonlSessionInfoEntry, JsonlMessageEntry, StatsDelta, and all associated functions (message_signature, build_jsonl_key, build_metadata_json, compute_stats_delta, read_jsonl_message_entries, read_jsonl_session_name, resolve_jsonl_message_indices, fetch_last_hstry_idx, resolve_jsonl_session_title). This is the largest concentration of dead code - the entire old Pi session management that was superseded by the runner architecture.

### [octo-nqg8] Eliminate all Rust warnings and unsafe code in backend (P1, epic)
Fix all ~398 compiler warnings in the backend workspace without bandaid fixes (#[allow], etc). This includes removing ~201 dead code items, replacing ~15 unsafe blocks with safe alternatives, fixing 164 ts-rs serde parse warnings, removing unused imports, and addressing unused variables. All changes must preserve existing functionality - dead code should only be removed if truly unused, not just stubbed.

### [octo-9cdt] Make hstry the sole history source while preserving streaming (P1, epic)

### [octo-wg67] Implement @@ cross-agent routing in runner (P1, task)

### [octo-1hbb] Define runner session state machine and command dispatch (P1, task)

### [octo-y5r3] Octo migration: replace all direct SQLite access with hstry gRPC client calls (P1, epic)

### [octo-xxe2] Per-workspace hstry/mmry stores + sync scoping (P1, task)
Ensure each workspace has isolated hstry/mmry stores; sync and cache are workspace-scoped with location_id/actor metadata. No cross-workspace leakage.

Note: hstry-core now has Sender/SenderType types in parts.rs (see hstry trx-7fh3, completed) for message attribution. This can be used for multi-user workspace scenarios.

### [octo-wdkj] Remote runner bootstrap over SSH (P1, task)
Implement SSH bootstrap: install deps, download runner binaries, configure sandbox, start service, register with hub. Provide fallback to bundle+push.

### [octo-59py] Workspace locations schema + routing (P1, task)
Add workspace_locations (workspace_id, runner_id, path, kind, repo_fingerprint, active flag) and route requests by selected location. Default to local if present; prompt on failure.

### [octo-pdb4] Shared workspaces with multi-location runners (P1, epic)
Support team shared workspaces with per-workspace hstry/mmry, local+remote locations, and explicit agent targeting. Includes runner bootstrap over SSH, workspace location routing, UI grouping/merge-split, and security model for remote execution + history sync.

### [octo-1ddx] Move global sandbox config to read-only path (P1, task)
Global sandbox config must be read-only (not user-writable). Implement loading from system path (e.g., /etc/octo/sandbox.toml) with user config for overrides or removal of user-writable global config. Ensure sandbox.toml itself is protected and update docs/install.

### [octo-t2bf] Multi-Runner & Workspace Sharing (P1, epic)
Epic for enabling users to connect to multiple runners across different machines (laptop, desktop, cloud) and share workspaces with other users.

## Goals

### Multi-Runner Support
...


### [octo-zjs8.3] Add strict clippy lints to all Cargo.toml files (P1, task)
Add workspace-level clippy configuration to deny warnings and enforce best practices

### [octo-zjs8.2] Fix clippy warnings in frontend/src-tauri (P1, task)
Fix unnecessary_lazy_evaluations, manual_flatten, single_match, and manual_strip warnings

### [octo-qq9y] Security audit sudoers configuration for multi-user mode (P1, task)
## Background

The sudoers configuration in setup.sh had critical security vulnerabilities that could allow privilege escalation. These have been fixed.

### Vulnerabilities Found and Fixed
...


### [octo-p3n2] API Key Authentication & External Integration (P1, epic)
Enable external apps (omni, ctx) to integrate with Octo via API keys. Support fire-and-forget and streaming responses, .ctx context files, auto-session creation.

### [octo-xjs5.10] Test plan: isolation matrix (P1, task)
Add automated tests / manual checklist for:
- local single-user
- local linux multi-user (2 users) verifying no cross access to sessions/files/memories/main chat
- container multi-user

...


### [octo-xjs5.7] Local linux provisioning: non-interactive sudo + tmpfiles (P1, task)
Make user provisioning deterministic and non-interactive:
- ensure /run/octo/runner-sockets exists via tmpfiles
- ensure per-user runner directories (2770, setgid) are created at user creation
- ensure linger + octo-runner user unit enabled
- verify runner socket reachable after provisioning
...


### [octo-xjs5.6] Security: runner socket authz + strict path guards (P1, task)
Harden the runner boundary:
- unix socket permissions via /run/octo/runner-sockets/<linux_username>/octo-runner.sock (group octo)
- runner validates every request against its own user roots
- deny dangerous env vars, validate binary allowlist
- ensure backend cannot ask runner to read outside workspace/main-chat dirs
...


### [octo-xjs5.5] Backend: route all user operations through runner (P1, task)
Refactor backend handlers/proxies so all user-plane operations are served via runner:
- sessions list/create/resume/stop
- terminal + opencode process lifecycle
- workspace file viewer
- memories (mmry target resolution)
...


### [octo-xjs5.4] Runner API: main chat storage ownership (P1, task)
Make main chat data physically per-user:
- move main chat DB + session files into linux user's home dir
- runner provides APIs for main chat list/create/load/save
- migrate existing shared main chat data into per-user location

...


### [octo-xjs5.3] Runner API: per-user mmry lifecycle (P1, task)
Move mmry lifecycle/port ownership fully into runner:
- allocate stable per-user mmry port (persisted in per-user db)
- spawn/stop/pin mmry process for that linux user
- health check + log capture
- expose mmry external_api URL for backend proxy
...


### [octo-xjs5.2] Runner API: per-user file operations (P1, task)
Define + implement runner-side filesystem API used by the backend:
- list tree, read file, write file, mkdir, stat
- path validation against per-user workspace roots (no traversal)
- optional read/write quotas

...


### [octo-xjs5.1] Runner API: user-plane session registry (P1, task)
Add runner RPC endpoints for per-user session state:
- create/get/list/stop/resume/delete sessions (local runtime)
- persist session metadata in per-user store (DB or JSON) owned by the linux user
- include workspace path validation against runner-owned workspace roots
- return ports + status + errors
...


### [octo-thhx.2] Onboarding API endpoints (P1, task)
REST endpoints: GET/PUT /api/onboarding/state, POST /api/onboarding/unlock/{component}, POST /api/onboarding/godmode, POST /api/onboarding/complete

### [octo-thhx.1] Onboarding state model and database schema (P1, task)
Backend model for tracking onboarding progress, unlocked components, user level, and language preference. Store in user preferences table or dedicated onboarding_state table.

### [octo-thhx] Onboarding & Agent UI Control (P1, epic)
Progressive onboarding experience with wizard-driven UI, spotlight system, and i18n support. Onboarding uses step-by-step wizard forms, NOT agent-driven A2UI conversations.

### [octo-af5j.7.6] Release manifest generator (byt release) (P1, task)
byt release command that: 1) Reads component list from octo/release.toml, 2) Fetches current version from each repo (Cargo.toml, package.json, go.mod), 3) Generates versions.toml with all pinned versions, 4) Optionally tags all repos with octo-0.2.0 tag

### [octo-af5j.7.1] Version manifest file format (P1, task)
Define versions.toml or versions.json schema that lists all component versions for a release. Embedded in binary or fetched at runtime.

### [octo-af5j.7] Dependency version pinning and compatibility matrix (P1, feature)
Pin versions of opencode, pi, mmry, fileserver, ttyd, and other dependencies for each Octo release. Ensure all components are tested together. Include in release artifacts.

### [octo-af5j.3] Self-Update Command (P1, feature)
octoctl self-update command that downloads latest binary, verifies checksum, replaces current binary, and restarts service. Support for update channels (stable/beta).

### [octo-af5j.2] Template Repository & Versioning (P1, feature)
Separate templates into dedicated repo with version control. Enable template updates independent of binary releases. Track installed template versions per-agent.

### [octo-af5j.1] Binary Release Pipeline (P1, feature)
Build and distribute pre-compiled binaries for Linux (x86_64, arm64) and macOS (Intel, Apple Silicon). Eliminates need for Rust toolchain on user machines.

### [octo-af5j] Release & Update System (P1, epic)
Comprehensive system for distributing Octo releases, managing updates in the field, and expanding runtime options including Proxmox LXC support.

### [workspace-jux6.1] Lazy load main app components (P1, task)
Convert synchronous imports in apps/index.ts to React.lazy() imports. Currently SessionsApp, AgentsApp, ProjectsApp, SettingsApp, AdminApp are all bundled together. This blocks initial render with unused code.

Location: frontend/apps/index.ts:1-56

Implementation:
...


### [oqto-6e9v] Deploy SearXNG on octo-azure and configure sx for all users (P2, task)

### [oqto-mvdv.4] Session freshness fingerprinting for cross-device drift detection (P2, task)
Add session freshness polling to detect when the session file has been modified outside Octo's knowledge (e.g., by another browser tab or direct Pi CLI usage). Poll a fingerprint (mtime + size + entry count + tail hash) every ~4 seconds and warn the user when stale state is detected.

## Reference
Pi-mobile polls bridge_get_session_freshness every 4s with a 90s grace window for local mutations. On mismatch, shows 'Sync now' warning.

...


### [octo-s547] Pi extension: multi-account model switching with reasoning trace cleanup (P2, feature)

### [octo-nqg8.11] Fix unused imports and variables across backend (P2, task)
Fix ~14 remaining minor warnings:
- ~11 unused imports across multiple files (SenderType in octo-protocol, chat::get_runner_for_user, get_trx_issue, ContainerPiRuntime/LocalPiRuntime/PiProcess/PiRuntime/PiSpawnConfig/RunnerPiRuntime, proxy functions, UserHstryConfig/UserHstryManager, WorkspaceLocationRepository/WorkspaceLocation, WorkspaceMeta and related, warn, WsCommand)
- ~2 unused variables (agent_port, etc)
- ~1 unnecessary mut

...


### [octo-nqg8.10] Fix ts-rs serde attribute parse warnings in canon/types.rs (P2, task)
164 'failed to parse serde attribute' warnings from ts-rs when processing canon/types.rs. These are generated by ts-rs v10 when it encounters serde attributes it doesnt understand (like skip_serializing_if, is_default_format custom functions etc).

Options:
1. Update ts-rs to a version that handles these attributes (check if v11+ fixes this)
2. Add ts(skip) or ts-specific attributes alongside serde ones where needed
...


### [octo-eez0] octo-browser session is killed when sending instructions to chat (P2, bug)

### [octo-af5j.6.4] octo-setup web wizard (P2, feature)
Web-based setup wizard served by octo itself for first-run configuration. Triggered when octo starts with no config or incomplete config.

Flow:
- Deployment mode selection
- LLM provider + API key configuration
...


### [octo-af5j.6.3] octo-setup CLI wizard (P2, feature)
Expand octo-setup from config hydrator into interactive CLI wizard. Two modes:

1. **octo-setup wizard** (CLI) -- Interactive terminal wizard using dialoguer/inquire:
   - Deployment mode (development/production)
   - User mode (single/multi)
...


### [octo-3d6q] Slash command popup driven by get_commands (P2, task)

### [octo-vrzt] Remove MainChatPiService legacy code (P2, task)
MainChatPiService in src/main_chat/pi_service.rs is legacy. Everything now goes through the runner path. Has deep tendrils: used by ws_multiplexed.rs (get_messages), api/handlers/chat.rs, api/main_chat_pi.rs (REST handlers), api/routes.rs, api/state.rs, api/handlers/agent_ask.rs, api/handlers/agent_rpc.rs, pi_workspace.rs. Need to migrate all callers to use runner client.

### [octo-mkvc] Remove legacy canon/ module (from_pi, from_hstry, from_opencode) (P2, task)
The old canon/ module (src/canon/{mod,types,from_pi,from_hstry,from_opencode}.rs) is only used by hstry/convert.rs which imports pi_message_to_canon, CanonMessage, ModelInfo. Either inline those conversions into hstry/convert.rs or keep a minimal version. The canonical protocol now lives in octo-protocol crate.

### [octo-d0a5] Agent targeting for remote locations (P2, task)
Expose workspace+location targets to agent UI and API. Require explicit selection for remote execution; enforce per-location policies and logging.

### [octo-6nhg] Shared workspace UI grouping + merge/split (P2, task)
Group sessions by team and workspace, show locations nested. Add merge-by-repo default with split toggle; indicate local/remote and active location.

### [octo-jxn7] Add verbosity setting + persistence (P2, task)
Expose chat verbosity level (1-3) in frontend settings and persist to localStorage. Default to 3.

### [octo-rpxy] Chat verbosity levels for tool calls (P2, epic)
Add frontend verbosity levels for chat rendering. Level 3 = current verbose tool cards. Level 2 collapses consecutive tool calls into a single dropdown with tabbed icons and scroll+arrows when overflow. Level 1 TBD (minimal).

### [octo-wah4] Security: Runner authentication and TLS for network endpoints (P2, task)
Implement runner authentication and TLS for network endpoints:

1. Runner authentication
   - Token-based auth when runner connects to backend
   - Runner identity verification
...


### [octo-mj2r] Frontend: Shared workspace UI with owner indicators (P2, task)
Add shared workspace UI with owner indicators:

1. Workspace list enhancements
   - Show owner avatar/name for shared workspaces
   - Show permission level badge (Read/Write/Execute)
...


### [octo-w2zp] Frontend: Runner selector in session management UI (P2, task)
Add runner selector to session management UI:

1. Runner selector component
   - Dropdown to select runner for new sessions
   - Show runner status indicator (online/offline)
...


### [octo-888b] CLI: Workspace permission commands (grant, revoke, list, audit) (P2, task)
Add workspace permission management CLI commands:

1. octoctl workspace grant-permission
   - Grant user access to workspace
   - Options: --workspace, --user, --permission (read/write/execute), --expires
...


### [octo-cddx] CLI: Runner registration commands (register, list, status, unregister) (P2, task)
Add runner registration CLI commands:

1. octoctl runner register
   - Register a new runner
   - Options: --name, --endpoint (socket/address/url), --default
...


### [octo-72xf] Backend: Audit logging for cross-user access (P2, task)
Implement comprehensive audit logging:

1. Access log model
   - AccessLog struct (requesting_user, workspace_id, action, granted, timestamp)
   - DB schema for access_logs table
...


### [octo-cab0] SessionService: Support multi-runner with permission checks (P2, task)
Extend SessionService for multi-runner and permission checks:

1. Session model updates
   - Add runner_id to SessionInfo
   - Add workspace_id and owner_user_id
...


### [octo-kc4x] Backend: Extended RunnerClient with network endpoint support (P2, task)
Extend RunnerClient to support multiple endpoint types:

1. Add endpoint enum
   - RunnerEndpoint::UnixSocket(PathBuf) - existing local mode
   - RunnerEndpoint::NetworkAddress(SocketAddr) - new TCP
...


### [octo-b67r] Backend: Workspace permission system with DB schema (P2, task)
Implement workspace permission system:

1. Database schema
   - workspaces table (id, path, owner_user_id, runner_id)
   - workspace_permissions table (workspace_id, granted_user_id, permission, expires_at)
...


### [octo-60b1] Runner registry: Multiple runners per user with registration (P2, task)
Implement runner registry for managing multiple runners per user:

1. Data model
   - RunnerRegistration struct (id, user_id, name, endpoint, status, last_seen)
   - RunnerEndpoint enum (UnixSocket, NetworkAddress, SecureUrl)
...


### [octo-fcs1] Design: Multi-runner architecture and workspace permission model (P2, task)
Write comprehensive design document covering:

1. Multi-runner architecture
   - Runner registry data model
   - Runner registration flow
...


### [octo-7mxb] MCP Apps Support - Interactive UI in Chat (P2, epic)
Add support for MCP Apps extension to render interactive HTML interfaces (dashboards, forms, visualizations) directly in the chat UI. This enables richer user interactions beyond text/images - file browsers, build output viewers, deployment config forms, live metrics dashboards, etc.

## Phases

### Phase 1: Frontend Host Support
...


### [octo-zjs8.4] Optimize Rust compilation times (P2, task)
Add .cargo/config.toml with linker optimizations, split-debuginfo, incremental builds, and codegen-units settings

### [octo-7xx0] Note: octo-ssh-proxy socket path must be mounted or moved for sandbox access (P2, task)

### [octo-p3n2.6] .ctx file parsing (P2, task)
Parse .ctx zip files: extract images, text context, metadata. Store temporarily for agent access.

### [octo-p3n2.4] External chat API endpoint (P2, task)
POST /api/v1/chat - accepts message + optional .ctx file, auto-creates session for workspace, supports stream and fire_and_forget modes

### [octo-p3n2.3] Key management endpoints (P2, task)
POST /api/keys (create), GET /api/keys (list), DELETE /api/keys/{id} (revoke)

### [octo-p3n2.2] API key generation and validation (P2, task)
Generate prefixed keys (octo_sk_...), hash storage, validation in auth middleware alongside JWT

### [octo-p3n2.1] Database migration for api_keys table (P2, task)
Create SQLite migration with: id, user_id, name, key_prefix, key_hash, scopes, last_used_at, expires_at, created_at, revoked_at

### [octo-8erz] Prevent template copy from following symlinks (P2, bug)
copy_template_dir uses fs::copy on DirEntry paths without checking for symlinks; on most platforms this follows symlinks and can copy arbitrary files outside the template repo into the new project. This is a security risk if template repos are user-supplied. Use symlink_metadata to detect symlinks and either skip, copy as symlink, or enforce that resolved targets stay within the template repo. Affected: backend/crates/octo/src/api/handlers.rs::copy_template_dir.

### [octo-015j] Move blocking filesystem work out of async request handlers (P2, task)
Several async handlers call std::fs synchronously (read_dir/read_to_string/copy), which can block the Tokio runtime under load. Convert to tokio::fs or wrap in spawn_blocking. Examples: backend/crates/octo/src/api/main_chat_pi.rs::get_prompt_commands (read_dir/read_to_string), backend/crates/octo/src/api/handlers.rs::list_workspace_dirs (read_dir), list_project_templates (read_dir), find_project_logo (read_dir), copy_template_dir (read_dir/fs::copy).

### [octo-fmxv] Invalid `boundary` for `multipart/form-data` request when trying to save a file after editing it in the sidebar. (P2, bug)

### [octo-xjs5.9] Observability: runner logs + health endpoints (P2, task)
Add runner health + diagnostics:
- ping/status endpoint
- per-process status + stdout/stderr tail
- structured events for start/stop/crash
- backend exposes aggregated diagnostics for admins

### [octo-xjs5.8] Compatibility: container mode runner adapter (P2, task)
Keep docker multi-user working:
- define how runner API maps to container runtime
- either run runner inside container, or implement a backend adapter that satisfies runner interface using container APIs
- ensure streaming still works

...


### [octo-h0by] add user self-service section in settings (change password etc) (P2, task)

### [octo-85f4] the stop button doesnt seem to stop a running agent response (P2, bug)

### [octo-w85q] When adding an image to the canvas, we need to automatically fit the canvas to the image size. And the default canvas size should be 1280x1280 with a properly sized default font size (P2, bug)

### [octo-vbzq] Add Edit button to file viewer toolbar (P2, feature)
Add an Edit button with pencil icon to the file viewer toolbar, alongside the existing expand/collapse, search, and close panel buttons. The Edit button should open the file for editing.

### [octo-58xa.3] WebView: MCP tool for opening webviews (P2, task)
Add MCP tool for agents to open webviews.

## Tool: webview_open
Parameters:
  - url: string (required) - localhost:PORT or path
...


### [octo-58xa.2] WebView: Frontend iframe component (P2, task)
Create WebView React component for displaying agent web apps.

## Component: WebView
Props:
  - url: string
...


### [octo-58xa.1] WebView: Backend proxy for localhost servers (P2, task)
Create proxy endpoint for agent-spawned web servers.

## Endpoint
GET/POST /api/session/{id}/webview/proxy
Query params:
...


### [octo-58xa] Agent WebView: Iframe embed for agent-spawned web apps (P2, feature)
Allow agents to spawn custom web apps and display them in Octo's UI with sidebars visible.

## Use Cases
- Agent creates a data visualization dashboard
- Agent builds a custom form/wizard for user input
...


### [octo-k8z1.13] Browser: User interaction handoff mode (OAuth, captcha, 2FA) (P2, task)
When agent encounters OAuth, captcha, or 2FA, it needs to hand control to user.

## Flow
1. Agent detects auth page or blocker
2. Agent calls: browser_request_user_action({ reason: 'Please log in to GitHub' })
...


### [octo-thhx.16] Tutorial script using spotlight and A2UI (P2, task)
Optional guided tutorial using spotlight overlays to introduce UI components. Sequential step-by-step walkthrough, not agent-driven. User can skip at any time.

### [octo-thhx.15] Profile and personality setup conversation (P2, task)
Web wizard step for user profile and personality setup. Simple form fields for name, timezone, communication style, assistant name/personality. Writes USER.md and PERSONALITY.md. Not agent conversation.

### [octo-thhx.14] Provider setup wizard via A2UI (P2, task)
Web wizard step for connecting LLM providers. If EAVS pre-configured by admin, skip. Otherwise: show provider options, collect API key via form input, test connection, store in EAVS or user config. Simple multi-step form, not agent conversation.

### [octo-thhx.13] i18n AGENTS.md translations (P2, task)
Prepare AGENTS.md in multiple languages: en, de, es, fr, pl, etc. Either use symlinks (AGENTS.md -> AGENTS.{lang}.md) or dynamic injection based on user language preference.

### [octo-thhx.12] Progressive UI unlock system (P2, task)
Extend Features API with unlocked_components map. Components check unlock state before rendering. Unlock triggers: first message, tutorial progression, technical detection.

### [octo-thhx.11] Godmode command to skip onboarding (P2, task)
Implement /godmode slash command, Ctrl+Shift+G shortcut, and ?godmode=true URL param. Unlocks all UI components, marks onboarding complete, sets user level to technical.

### [octo-thhx.10] Onboarding route and flow controller (P2, task)
Dedicated /onboarding route that orchestrates: language selection -> provider setup -> profile conversation -> tutorial. Redirects new users here, remembers progress.

### [octo-thhx.9] Language selection word cloud with CRT shader (P2, task)
Three.js or CSS animated word cloud showing 'Click me' in multiple languages. CRT post-processing effect (scanlines, chromatic aberration, flicker). Click detection triggers language selection.

### [octo-thhx.8] Tour mode for sequential spotlights (P2, task)
Support multi-step tours with automatic progression. Agent sends array of steps, frontend advances on user click or timeout. Include progress indicator and skip button.

### [octo-smwr] Add drag an drop capabilities to the file tree, both for dragging in external files and for moving files between dirs  (P2, feature)

### [octo-s4ez] Define context model and context sources (local + remote) (P2, task)
## Goal
Define what "context" means for agent interactions in Octo, and how it is represented, versioned, and sourced, so features like global agent invoke (`octo-skks`) and agent-driven UI control (`octo-wzvn`) can reliably inject context now and later.

## Context Model (Proposed)
A versioned envelope composed of multiple context "sources".
...


### [octo-skks] Global main agent invoke with context injection (P2, feature)
## Problem\nUsers want to invoke the Main Agent from any page in the web app, and have the agent automatically receive UI/runtime context (current page/route, active app/view, selected agent/persona if applicable, selected workspace directory/project, current session IDs).\n\n## Proposed Feature\nAdd a globally-available Main Agent entrypoint (e.g., hotkey + floating button + command palette action) that opens the Main Chat/agent panel. When the user sends a message, inject a structured context block into the message/system prompt containing:\n- Current route/pathname\n- Active app/view (e.g. sessions/settings/admin)\n- Active agent/persona (if any)\n- Current workspace directory / project key\n- Current workspace session ID + current chat session ID (if available)\n\n## Acceptance Criteria\n- Main Agent can be opened from any page without navigation side effects.\n- Sent messages include the context injection reliably and deterministically.\n- Context injection is visible in logs/devtools (or can be toggled) for debugging.\n- Works when OpenCode is not running (falls back to disk/history context).\n- No regression to existing Main Chat / Sessions flows.\n\n## Notes\nImplementation likely touches: app shell routing, global UI overlay, and the message send pipeline (control-plane / opencode proxy headers).

### [octo-2r4f] Add slug field to session model and API responses (P2, task)

### [octo-6pkd] Left sidebar: '+' next to SESSIONS should create session in current project; add separate 'new directory/project' button (P2, feature)
UX change request:
- The "+" button next to "SESSIONS" in the left sidebar should open/create a new session/chat within the project the user is currently in.
- Add a separate "new dir" button for the existing create project functionality to make the distinction clearer.

Confirmed behavior:
...


### [octo-af5j.7.5] LXC template version tags (P2, task)
Same as container images but for LXC templates. octo-agent-0.2.0.tar.zst with pinned components.

### [octo-af5j.7.4] Container image version tags (P2, task)
Tag container images with Octo release version. octo-agent:0.2.0 contains exact pinned versions. Latest tag follows stable channel.

### [octo-af5j.7.3] Bundled component downloads (P2, task)
Release artifacts include or reference exact versions of opencode, pi, mmry, fileserver. Self-update fetches matching versions.

### [octo-af5j.7.2] Component version checking at startup (P2, task)
On startup, verify installed component versions match expected. Warn on mismatch, offer to update. Block startup on critical incompatibility.

### [octo-4me3] Security/perf/idiomatic audit fixes (P2, epic)
Bundle of findings from the comprehensive review; child issues are linked as blockers.

### [octo-af5j.4.7.5] LXC template download and preparation (P2, task)
Download octo-agent LXC template to local storage. Or build from Dockerfile equivalent. Pre-warm template cache.

### [octo-af5j.4.7.4] Octo backend installation on Proxmox host (P2, task)
Install octo binary, configure for Proxmox runtime mode, create API token for LXC management, set up systemd service.

### [octo-af5j.4.7.3] Storage configuration (P2, task)
Set up local-lvm or ZFS pool for container storage. Configure template storage location. Optional: add NFS/Ceph for shared storage.

### [octo-af5j.4.7.2] Network configuration for Proxmox (P2, task)
Configure vmbr0 bridge, optional NAT for agent containers, firewall rules. Support single NIC and multi-NIC setups.

### [octo-af5j.4.7.1] Proxmox VE installation automation (P2, task)
Add Proxmox repo to Debian, install pve-manager, configure grub for IOMMU if needed. Handle both fresh Debian and Proxmox ISO scenarios.

### [octo-af5j.4.7] Proxmox automated installer (P2, feature)
Script that takes a fresh Debian/bare-metal system, installs Proxmox VE, configures networking, and bootstraps Octo with LXC runtime. Single command to go from bare metal to running Octo instance.

### [octo-af5j.6.2] Platform detection and binary selection (P2, task)
Detect OS (Linux/macOS), arch (x86_64/arm64), libc (glibc/musl). Download matching binary. Fallback to source build if no binary.

### [octo-af5j.6.1] One-liner install command (P2, task)
curl -fsSL https://octo.ai/install.sh | sh - Downloads binary, adds to PATH, runs initial setup wizard.

### [octo-af5j.5.2] Migration scripts framework (P2, task)
Embedded migration functions (v0->v1, v1->v2, etc). Apply in order. Track applied migrations.

### [octo-af5j.5.1] Config version detection (P2, task)
Add version field to config.toml. Detect missing version as v0. Warn on unknown version.

### [octo-af5j.6] Installation Script Overhaul (P2, feature)
Rewrite setup.sh to download pre-built binaries instead of compiling. Detect platform, fetch correct binary, install to PATH. One-liner install like rustup.

### [octo-af5j.5] Config Migration System (P2, feature)
Detect config.toml version, apply migrations for breaking changes. Backup before migrating. Support dry-run mode.

### [octo-af5j.4.3] RuntimeBackend trait implementation for Proxmox (P2, task)
Implement create/start/stop/exec/logs for LXC. Map agent sessions to VMID range. Handle networking (bridge, NAT, port forwarding).

### [octo-af5j.4.2] LXC template for agent containers (P2, task)
Minimal LXC template with opencode, fileserver, ttyd pre-installed. Based on Arch or Alpine. Published to Proxmox template storage.

### [octo-af5j.4.1] Proxmox API client (P2, task)
Rust client for Proxmox REST API. Authentication (API tokens), node discovery, LXC CRUD operations, exec/console access.

### [octo-af5j.3.3] Service restart orchestration (P2, task)
Gracefully stop running sessions, replace binary, restart systemd/launchd service. Handle in-flight requests. Rollback on failure.

### [octo-af5j.3.2] Binary download and verification (P2, task)
Download correct platform binary, verify SHA256 checksum, optionally verify GPG signature. Atomic replacement of current binary.

### [octo-af5j.3.1] Update check endpoint (P2, task)
GitHub API or dedicated endpoint to check latest version. Compare with installed version. Cache results to avoid rate limits.

### [octo-af5j.2.4] Agent template version tracking (P2, task)
Store template version used when creating agent. Enable 'octoctl agent update-templates' to upgrade individual agents.

### [octo-af5j.2.3] octoctl templates command (P2, task)
CLI for template management: list, install, update, diff. Track installed versions in ~/.config/octo/templates.lock

### [octo-af5j.2.2] Template manifest format (P2, task)
Define manifest.json schema: template metadata, version, compatibility range, variables/placeholders, dependencies between templates.

### [octo-af5j.2.1] Create octo-templates repository (P2, task)
New repo with versioned templates: AGENTS.md variants, opencode.json presets, plugins, scaffold templates. Semantic versioning independent of octo core.

### [octo-af5j.1.4] Version embedding in binaries (P2, task)
Embed git tag/commit in binaries at build time. octo --version shows semver + commit hash.

### [octo-af5j.1.3] GitHub Releases integration (P2, task)
Automate publishing to GitHub Releases on tag push. Generate changelog from commits. Upload all platform artifacts.

### [octo-af5j.1.2] Release artifact packaging (P2, task)
Package binaries as tarballs with install script, checksums (SHA256), and signatures. Include octo, octoctl, fileserver binaries.

### [octo-af5j.1.1] GitHub Actions workflow for cross-compilation (P2, task)
CI workflow using cross-rs or native runners to build for linux-x86_64, linux-arm64, darwin-x86_64, darwin-arm64

### [octo-af5j.4] Proxmox LXC Runtime (P2, feature)
New runtime backend using Proxmox API to provision LXC containers for agent sessions. Stronger isolation than Docker, native systemd support, persistent containers with snapshots.

### [octo-fxhc] Stream zip downloads to avoid large in-memory buffers (P2, task)
fileserver/src/handlers.rs: create_zip_from_paths reads entire files and builds zip data in a Vec<u8>, which can exhaust memory for large files/directories. Consider streaming zip output or enforcing a size limit with early abort.

### [octo-1rb4.1] Make turn taking more robust (P2, task)

### [octo-wmrf.5] MCP Tool: a2ui_surface (P2, task)
Create MCP tool for agents to emit A2UI surfaces. Parameters: surface_id, messages (A2UI JSON), blocking (bool). Non-blocking returns immediately, blocking waits for userAction response. Works for OpenCode agents, Pi agent, and future CLI agents.

### [octo-374f] Cache-aware conversation compaction for Main Chat (P2, feature)
Implement smart compaction that preserves LLM cache benefits. Key strategies: 1) Hide tool calls in UI but keep in API payload, 2) Tiered compaction (hot/warm/cold zones), 3) Append-only summarization at checkpoints, 4) Provider-aware caching (OpenAI auto vs Anthropic explicit). UI shows collapsed/expandable tool calls. Research needed on optimal checkpoint intervals and summary strategies.

### [octo-jrya] Notification system for agents and external events (P2, feature)
Add a notification system to Octo that agents can push to via HTTP API. Includes: SQLite storage, REST API (POST /api/notify, GET /api/notifications, etc.), WebSocket broadcast to frontend, right sidebar tab with notification list, optional popup toasts, ntfy.sh integration for external push. CLI: curl-based for agents. Config in octo settings for ntfy URL/token.

### [octo-7fms] Main Chat: Enhanced compaction (observation masking, 8-section summary) (P2, task)
Enhance Main Chat compaction with two-phase approach:

Phase 1: Observation Masking (cheap, zero tokens)
- Replace old tool results with placeholder: [Previous output elided for brevity]
- Preserves: system prompt, recent N messages, file state
...


### [workspace-jux6.5] Add service worker for static asset caching (P2, task)
No service worker exists. Static assets (JS/CSS bundles) are re-fetched on every visit.

Implementation:
- Add vite-plugin-pwa dependency
- Configure workbox to cache static assets
...


### [workspace-jux6.4] Prefetch critical data during idle time (P2, task)
Critical data like workspace sessions and chat history is fetched on-demand, causing delays when navigating.

Location: frontend/components/app-context.tsx

Implementation:
...


### [workspace-jux6.3] Add manual chunks to Vite build config (P2, task)
No explicit chunking strategy exists. Vite bundles everything with default splitting which is suboptimal.

Location: frontend/vite.config.ts

Implementation:
...


### [workspace-5pmk.9] Configure iOS and Android targets (P2, task)
Run tauri ios init and tauri android init. Configure permissions (microphone, network), app icons, splash screens, and build settings for both platforms.

### [workspace-5pmk.8] Add mobile-responsive UI adjustments (P2, task)
Ensure touch targets (44px min), safe areas (notch/home indicator), and gestures work on mobile. Test terminal and voice mode UX on touch devices.

### [workspace-5pmk.7] Implement Tauri main with backend startup (P2, task)
Start Axum backend on app launch, configure webview to load from backend URL. Handle graceful shutdown.

### [workspace-5pmk.5] Update voice URL resolution for proxy mode (P2, task)
Detect Tauri/proxied mode, use relative WebSocket paths (/api/voice/stt, /api/voice/tts) instead of direct URLs from config.

### [workspace-5pmk.4] Remove server-side auth from frontend (P2, task)
Delete middleware.ts, implement client-side auth guard in app layout. Check auth_token cookie on mount, redirect to /login if invalid.

### [workspace-5pmk.3] Configure frontend for static export (P2, task)
Set output: export in next.config.ts, disable image optimization, remove rewrites (backend handles routing).

### [workspace-5pmk.1] Add static file serving to backend (P2, task)
Use tower_http::services::ServeDir to serve frontend static export from /, with SPA fallback to index.html. Enables: (1) single-binary deployment without separate web server, (2) webapp mode without Next.js server, (3) simpler CORS since everything is same-origin.

### [workspace-gg16.7] Local mode: Per-user mmry service management (P2, task)
For local mode, spawn mmry service per Linux user (similar to opencode). Use user's home directory for database. Generate user-specific config with remote embedding delegation. Add to session lifecycle (start/stop).

### [workspace-gg16.5] Frontend: Memories tab UI components (P2, task)
Build Memories tab with Radix UI components. MemoryList (paginated, sortable), MemorySearch (query input, mode selector, rerank toggle), MemoryCard (content, category, tags, importance, date), MemoryEditor (add/edit form with validation), StoreSelector (per-repo stores).

### [workspace-gg16.4] Frontend: React Query hooks for memories API (P2, task)
Create TanStack Query hooks: useMemories, useMemorySearch, useCreateMemory, useUpdateMemory, useDeleteMemory. Handle pagination, optimistic updates, error states. Type definitions for Memory objects.

### [workspace-x7gm.5] Frontend: Project management UI (P2, task)
Add UI for:
- Creating and managing projects
- Inviting users to projects
- Switching between personal workspaces and shared projects

### [workspace-x7gm.4] Update AgentBackend to support project-based sessions (P2, task)
Modify LocalBackend and ContainerBackend to:
1. Check if workspace path belongs to a project
2. Use project's Linux user instead of platform user's Linux user
3. Store session data under project user's directory

### [workspace-x7gm.3] Add project management API endpoints (P2, task)
API endpoints:
- POST /projects - Create project
- GET /projects - List user's projects
- GET /projects/{id} - Get project details
- PUT /projects/{id} - Update project
...


### [workspace-x7gm.2] Implement Project service and repository (P2, task)
Create ProjectRepository and ProjectService for CRUD operations on projects and memberships.

### [workspace-x7gm.1] Add projects and project_members tables to database (P2, task)
Create migrations for projects and project_members tables as defined in the epic.

### [workspace-x7gm] Shared Projects: Multi-user access to same project/workspace (P2, epic)
Enable multiple platform users to access the same project/workspace with proper isolation.

## Design

### Core Concept
...


### [oqto-v9g0] Files panel shows stale file tree when switching between sessions in same workspace (P3, bug)

### [oqto-mvdv.5] WebSocket performance budget tracking and diagnostics (P3, task)
Add performance budget tracking for key streaming metrics:
- TTFT (prompt send to first text_delta): target 1200ms
- Reconnect-to-resync latency: target 2000ms
- Messages-per-second throughput during streaming
- Inbound queue depth high-water mark per session
...


### [octo-34f0] Migrate generate_pi_models_json to eavs API endpoint (P3, chore)
The octo backend has a duplicate of the Pi models.json generation logic in backend/crates/octo/src/eavs/mod.rs. Now that eavs has 'eavs models export pi' (and the export module), the octo backend should call an eavs API endpoint (e.g. GET /providers/export/pi) instead of duplicating the format logic. This would be a new API endpoint in eavs that returns the same JSON as the CLI command.

### [octo-qqg2] Support octo.install.toml for non-interactive setup with provider API keys (P3, feature)
Add support for reading an octo.install.toml file that pre-configures LLM providers and API keys for non-interactive setup. The file would be read at the start of setup.sh and converted into environment variables. This enables fully automated deployments without manual key entry. Format:
```toml
[providers]
anthropic = "sk-ant-..."
openai = "sk-..."
...


### [octo-wfs3] Expand fuzz testing to WebSocket commands, runner protocol, and Pi event parsing (P3, task)

### [octo-mxd8.4] macOS fallback: socket broker for guarded paths (P3, feature)
Implement a non-FUSE fallback for macOS that provides similar functionality to octo-guard using a socket-based broker.

## Overview
Since FUSE on macOS is problematic (kext deprecation, SIP issues), implement a simpler socket+copy approach.

...


### [octo-xncy.7] Android: UI exploration mode (DroidBot-style UTG crawler) (P3, task)
Build UI exploration/crawling mode for unknown apps.

## Approach
DroidBot-style UI Transition Graph (UTG) crawler:
1. Systematically explore app screens
...


### [octo-xncy.6] Android: MCP tools for agent control (snapshot, tap, type, scroll) (P3, task)

### [octo-xncy.5] Android: Frontend AndroidView component with touch forwarding (P3, task)

### [octo-xncy.4] Android: Screencast streaming to frontend (reuse BrowserView pattern) (P3, task)

### [octo-xncy] Android Emulator: Agent-controlled Android environment (P3, epic)
-

### [octo-mbeh] Deduplicate and centralize path sanitization logic (P3, task)
There are multiple path sanitization/validation implementations with overlapping intent (e.g., sanitize_relative_path in API handlers vs resolve_path/resolve_and_verify_path in the file server). This risks divergence and inconsistent security rules. Consider centralizing into a shared utility with shared tests. Affected: backend/crates/octo/src/api/handlers.rs::sanitize_relative_path, backend/crates/octo-files/src/handlers.rs::resolve_path/resolve_and_verify_path.

### [octo-a256] Consolidate CopyButton implementations and handle clipboard failures consistently (P3, task)
CopyButton logic is duplicated across multiple components with inconsistent error handling and timer cleanup (e.g., missing try/catch and no timeout cleanup on unmount). Consider a shared CopyButton component/hook with fallback copy logic and timeout cleanup. Affected: frontend/components/ui/markdown-renderer.tsx, frontend/components/ui/code-viewer.tsx, frontend/components/ui/typst-viewer.tsx, frontend/apps/admin/InviteCodesPanel.tsx, frontend/features/sessions/SessionScreen.tsx, frontend/features/main-chat/components/MainChatPiView.tsx.

### [octo-3trr] Add browser extension mode (Option A) - fork Playwriter (P3, feature)
Browser extension mode for controlling user's existing browser.

## Reference Implementation
- ../external-repos/playwriter - Fork this for Octo extension
- ../external-repos/clawdbot/src/browser/extension-relay.ts - CDP relay pattern
...


### [octo-thhx.18] Multi-lingual user support (P3, task)
Support users who speak multiple languages. Store languages array in USER.md. Agent can switch language based on context or explicit request. UI for managing language preferences.

### [octo-thhx.17] Technical user detection for terminal unlock (P3, task)
Subtle detection: profile questions about work, A2UI choice between visual vs command options, detection of shell-like input in chat. Unlocks terminal for technical users.

### [octo-k8z1.12] Documentation: Browser feature usage guide (P3, chore)

### [octo-k8z1.11] Container mode: Browser container per session (Docker/Podman) (P3, task)

### [octo-k8z1.10] Human-in-the-loop: Credential access approval modal (P3, task)

### [octo-k8z1.9] Credential vault: UI for storing encrypted credentials (P3, task)

### [octo-af5j.4.7.6] First-run wizard for Proxmox+Octo (P3, task)
Interactive or config-file based wizard: set admin password, configure EAVS API keys, set resource limits, create first user/agent.

### [octo-af5j.4.6] GPU passthrough for LXC (P3, task)
Pass NVIDIA GPU to LXC containers for local LLM inference. Share GPU across multiple containers.

### [octo-af5j.4.5] Proxmox cluster support (P3, task)
Distribute agent containers across cluster nodes. Handle migration. Resource balancing.

### [octo-af5j.4.4] LXC snapshot support (P3, task)
Create snapshots before risky operations. Rollback on failure. Scheduled snapshots for long-running agents.

### [octo-af5j.3.4] Update channels (stable/beta/nightly) (P3, task)
Support multiple release channels. Config option to set preferred channel. Beta gets release candidates, nightly gets every commit.

### [octo-vne5] Move simple tree file walk off the async runtime (P3, task)
fileserver/src/handlers.rs: get_simple_file_list uses WalkDir synchronously on the async thread. On large directories this can block the runtime and slow all requests. Run this in spawn_blocking or switch to an async walker.

### [octo-wmrf.7] A2UI Custom Components Catalog (P3, task)
Define Octo-specific A2UI component catalog extending standard catalog. Custom components: CodeBlock (syntax highlighted), DiffView, FileTree, ProgressBar, Terminal, MarkdownView. Register with renderer, document for agent use.

### [octo-a9ds] Main Chat: Agent coordination via mailz (P3, task)
Enable agent coordination for Main Chat:

1. mailz integration (messaging only):
   - Check inbox on session start
   - Send messages to other agents (e.g., govnr)
...


### [octo-a9mc] Main Chat: skdlr heartbeat integration (P3, task)
Integrate skdlr for periodic heartbeats in Main Chat:

1. skdlr schedule configuration:
   - Heartbeat schedule (e.g., every 4 hours)
   - Command: octo main-chat heartbeat
...


### [workspace-ufvs] Integrate qmd for document search (P3, task)
Add qmd (tobi/qmd) as optional document search backend. qmd excels at hybrid search (BM25 + Vector + Query Expansion + Re-ranking) for existing knowledge bases, meeting notes, docs. Complementary to mmry which handles agent memories. Consider: MCP server integration, collection management, hybrid with mmry for different use cases.

### [workspace-jux6.6] Make i18n loading async (P3, task)
initI18n() is called synchronously in main.tsx:8, blocking React render until translations load.

Location: frontend/src/main.tsx:8

Implementation:
...


### [workspace-5pmk.10] Add platform-specific native features (P3, task)
Desktop: window management, system tray, keyboard shortcuts. Mobile: haptic feedback, safe area insets, native share. Use Tauri plugins and conditional compilation.

### [octo-xncy.9] Android: Vision fallback with OmniParser/grounding model (P4, task)

### [octo-gpj7] Avoid unwrap on WS event serialization (P4, chore)
backend/src/ws/handler.rs and backend/src/ws/types.rs use serde_json::to_string(...).unwrap(). A serialization failure would panic the server. Use map_err/Result and return an error response instead, even if failure is unlikely.

### [octo-92yw] Deduplicate mmry proxy session checks (P4, chore)
backend/src/api/proxy.rs: proxy_mmry_* handlers repeat the same session lookup + active check + target/store resolution. Factor into a helper to reduce duplication and keep behavior consistent when rules change.

### [octo-h975] Main Chat: Message visibility filtering (hide tools by default) (P4, task)
Filter message visibility in Main Chat to show cleaner output:

Current behavior: All messages (including tool calls) visible in chat
Desired behavior: Tool calls hidden by default, toggle to show

...


## Closed

- [oqto-f1fw] Configure exa MCP for Pi on octo-azure (install extension, set API key) (closed 2026-02-21)
- [oqto-y475] Active Session from 1970: timestamp display bug for sessions without proper created_at (closed 2026-02-21)
- [oqto-3t2d] Admin scripts: oqto-admin for server maintenance tasks (closed 2026-02-19)
- [oqto-hgrs] oqto-browser: auto-start browserd daemon and per-crate install recipes (closed 2026-02-19)
- [oqto-mvdv.3] Delta coalescing for streaming text/thinking updates (closed 2026-02-19)
- [oqto-mvdv.2] Deterministic resync after reconnect for active sessions (closed 2026-02-19)
- [oqto-mvdv.1] Backpressure detection and controlled reconnect in ws-manager.ts (closed 2026-02-19)
- [octo-k8z1.8] Session management: Browser lifecycle (start/stop with session) (closed 2026-02-17)
- [octo-k8z1.5] Frontend: Add browser tab to central pane view switcher (closed 2026-02-17)
- [octo-k8z1.2] Backend: WebSocket proxy for screencast stream (closed 2026-02-17)
- [octo-k8z1.1] Backend: Integrate agent-browser daemon per session (closed 2026-02-17)
- [octo-k8z1] Add server-side browser feature (Option B) using agent-browser (closed 2026-02-17)
- [octo-vn0r] Admin API: eavs provisioning in create_user and sync_user_configs (closed 2026-02-17)
- [octo-zqyg] Fix Linux user creation sudo allowlist path mismatch (closed 2026-02-17)
- [octo-9bqx] Add limits to zip download endpoints to prevent disk/CPU exhaustion (closed 2026-02-17)
- [octo-k9sp] Restrict token query auth to WebSocket-only paths (closed 2026-02-17)
- [octo-zjs8.1] Fix all clippy warnings in backend crates (closed 2026-02-16)
- [octo-1194] Remove OpenCode harness code (closed 2026-02-15)
- [octo-1194.6] Phase 5: Remove agent and session service OpenCode references (closed 2026-02-15)
- [octo-1194.10] Phase 9: Regenerate TypeScript types and final cleanup (closed 2026-02-15)
- [octo-1194.9] Phase 8: Clean up remaining scattered OpenCode references (closed 2026-02-15)
- [octo-1194.8] Phase 7: Delete opencode-client.ts and useOpenCode.ts (closed 2026-02-15)
- [octo-1194.7] Phase 6: Rename opencode-client.ts types to generic names (closed 2026-02-15)
- [octo-1194.5] Phase 4: Remove local/runtime.rs OpenCode config (closed 2026-02-15)
- [octo-1194.4] Phase 3: Remove OpenCode session spawning from runner (closed 2026-02-15)
- [octo-1194.3] Phase 2: Remove agent_rpc module (closed 2026-02-15)
- [octo-1194.2] Phase 1: Remove OpenCode proxy routes and SSE (closed 2026-02-15)
- [octo-1194.1] Phase 0: Delete dead WS and adapter files (closed 2026-02-15)
- [octo-af5j.6.5] setup.sh: EAVS mandatory LLM proxy + octo user home dir (closed 2026-02-13)
- [octo-62c1] Chat sessions: zombie entries, wrong titles, empty responses after reattach (closed 2026-02-13)
- [octo-e8q8] There is no way to display closed trx issues (closed 2026-02-13)
- [octo-hz24] bash tool calls are duplicated in the chat i.e. the same bash call always appears twice (closed 2026-02-10)
- [octo-rzc2] Missing icons in chat session list - icons not displaying for each item (closed 2026-02-10)
- [octo-78dz] Model switcher missing models: runner env file support (closed 2026-02-09)
- [octo-sde4] Model switcher broken: empty model list + 1-second polling (closed 2026-02-09)
- [octo-nsjj] Unable to switch models in chat session (closed 2026-02-07)
- [octo-f6yn] User message echoed in agent's streaming response bubble (closed 2026-02-07)
- [octo-xpb8] Remove legacy main_chat module - full removal from main.rs, state.rs, and handlers (closed 2026-02-07)
- [octo-z5dx] Update octo-protocol to re-export Sender from hstry-core (closed 2026-02-05)
- [octo-3486] WebSocket: Add runner_id and workspace_id to protocol types (closed 2026-02-05)
- [octo-r6pc] Fix chat session normalization and pending IDs (closed 2026-02-05)
- [octo-1cqd] Normalize chat workspace path and default assistant mapping (closed 2026-02-05)
- [workspace-gg16.2] Per-user mmry instance management (closed 2026-02-04)
- [workspace-5pmk.11] Add backend URL configuration to login form (closed 2026-02-04)
- [octo-3fkc] Use hstry canonical history + Pi export for rehydrate (closed 2026-02-04)
- [octo-f50n] redirect to login for unauthenticated users (closed 2026-02-04)
- [octo-ze9k] Dashboard with overview of scheduled tasks (skdlr), session information, trx issues etc. Similar to the admin panel but for all users (closed 2026-02-04)
- [octo-p3n2.5] Sessions listing endpoint (closed 2026-02-04)
- [workspace-5pmk.6] Create Tauri project structure (closed 2026-02-04)
- [workspace-gg16.3] Backend: Add mmry proxy API routes (closed 2026-02-04)
- [octo-gj7p] Integrate hstry-core for message persistence (closed 2026-02-04)
- [octo-95x0] Remove main_chat.db and duplicate message types (closed 2026-02-04)
- [workspace-5pmk.2] Add voice WebSocket proxies to backend (closed 2026-02-04)
- [workspace-4eyc] Main Chat: JSONL export and backup (closed 2026-02-04)
- [octo-jwc4] Unify Pi chats (default + workspace) (closed 2026-02-04)
- [octo-zd43] Collapse consecutive tool calls into tabbed dropdown (closed 2026-02-04)
- [octo-70pz] Add session naming - auto-generate from first message, allow editing (closed 2026-02-04)
- [octo-9qkv] Improve opencode chat error notifications (top-right toast) (closed 2026-02-04)
- [octo-aexm] message order gets mixed up in opencode session: earlier message shows up as latest message. (closed 2026-02-04)
- [octo-8pfr] 503 Service Unavailable on /code/ endpoints during agent reconnection (closed 2026-02-04)
- [octo-y1nq] Opencode agent connection cycling - rapid disconnect/reconnect loop (closed 2026-02-04)
- [octo-cvy0] Configurable chat prefetch limit (closed 2026-02-04)
- [octo-zjs8] Rust Code Quality & Compilation Optimization (closed 2026-02-04)
- [octo-qs75] Raw commands like ({"command":"ls -la"}) appear in pi chat messages (closed 2026-02-02)
- [octo-016a] Full component rerender on key event for trx issue input in sidebar (closed 2026-02-02)
- [octo-v2d4] Trx sidebar component remounts when adding a new issue in the sidebar (closed 2026-02-02)
- [octo-jdt5] File attachments don't work in pi chats  (closed 2026-02-02)
- [octo-p09v] tool calling spinner doesn't stop. all tool calls continue spinning (closed 2026-02-02)
- [octo-047z] Responses are duplicated in main chat (closed 2026-02-02)
- [octo-1j5m] Background history refresh races with session switches (closed 2026-01-31)
- [octo-pwnn] Backend Pi process reuse broadcasts events across sessions (closed 2026-01-31)
- [octo-dxsg] WebSocket handler swapping causes message leaks (closed 2026-01-31)
- [octo-p7v5] Messages saved with stale pi_session_id when switching sessions (closed 2026-01-31)
- [octo-jgc6] WebSocket messages lack session_id validation (closed 2026-01-31)
- [octo-k5yx] Data isolation bug: user can see other users' data if Linux user creation fails (closed 2026-01-31)
- [octo-6nsz] Sudoers OCTO_USERADD pattern does not match backend useradd command format (closed 2026-01-31)
- [octo-my07] Codebase Refactoring for Maintainability (closed 2026-01-30)
- [octo-my07.12] Add comprehensive test infrastructure (closed 2026-01-30)
- [octo-my07.13] Extract shared UI components to component library (closed 2026-01-30)
- [octo-my07.11] Refactor dashboard into feature components (closed 2026-01-30)
- [octo-my07.10] Implement unified error handling in backend (closed 2026-01-30)
- [octo-my07.9] Split session-context into focused contexts (closed 2026-01-30)
- [octo-my07.8] Implement TypeScript type generation from Rust (closed 2026-01-30)
- [octo-my07.4] Decompose usePiChat hook into focused hooks (closed 2026-01-30)
- [octo-my07.7] Standardize backend domain module structure (closed 2026-01-30)
- [octo-my07.6] Establish feature-based frontend organization (closed 2026-01-30)
- [octo-my07.5] Implement generic proxy factory in backend (closed 2026-01-30)
- [octo-my07.3] Split handlers.rs into domain-specific modules (closed 2026-01-30)
- [octo-my07.2] Extract AppShellRoute into focused components (closed 2026-01-30)
- [octo-my07.1] Split control-plane-client.ts into domain modules (closed 2026-01-30)
- [octo-c62h] Chat history: include Pi workspace sessions + Pi model in status bar (closed 2026-01-29)
- [octo-mxd8] Sandbox Security Enhancements: Custom Profiles, FUSE Guard, SSH Proxy (closed 2026-01-29)
- [octo-mxd8.2] Implement octo-guard FUSE filesystem for runtime access control (closed 2026-01-29)
- [octo-mxd8.3] Implement octo-ssh-proxy for controlled SSH agent access (closed 2026-01-29)
- [octo-mxd8.6] Implement prompt system for security approvals (closed 2026-01-29)
- [octo-mxd8.1] Custom sandbox profiles: update example and docs (closed 2026-01-29)
- [octo-mxd8.5] Network proxy for granular network access control (closed 2026-01-29)
- [octo-tbsf] Security: Session services were binding to 0.0.0.0 instead of 127.0.0.1 (closed 2026-01-26)
- [octo-xjs5] Runner As Core User-Plane (closed 2026-01-26)
- [octo-wbyq] Performance: eliminate >50ms UI handlers (closed 2026-01-25)
- [octo-psdq] text input boxes rerendering the entire component on every key stroke (closed 2026-01-25)
- [octo-xncy.1] Android: Emulator lifecycle management (AVD/Cuttlefish per session) (closed 2026-01-25)
- [octo-xncy.8] Android: Network traffic interception (mitmproxy for API discovery) (closed 2026-01-25)
- [octo-xncy.16] Android: System settings get/set and permission management (closed 2026-01-25)
- [octo-xncy.15] Android: Frida runtime hooking for API tracing (closed 2026-01-25)
- [octo-xncy.14] Android: Logcat streaming and parsing (closed 2026-01-25)
- [octo-xncy.13] Android: Intent/broadcast injection and activity inspection (closed 2026-01-25)
- [octo-xncy.12] Android: Content Provider queries (contacts, sms, calendar, media) (closed 2026-01-25)
- [octo-xncy.11] Android: Storage access (SQLite, SharedPrefs, files pull/push) (closed 2026-01-25)
- [octo-xncy.3] Android: UI tree extraction via accessibility service (closed 2026-01-25)
- [octo-xncy.2] Android: ADB control API (tap, type, scroll, back, home) (closed 2026-01-25)
- [octo-xncy.10] agent-android: Core CLI/daemon scaffold (Rust) (closed 2026-01-25)
- [octo-eb0b] Per-user mmry instances in local multi-user mode (closed 2026-01-24)
- [octo-vvn7] Define runner user-plane RPC API (closed 2026-01-23)
- [octo-wzvn] Agent-driven UI control (conversational navigation) (closed 2026-01-21)
- [octo-q9dx] Transcription continuing after stopping Conversation mode (closed 2026-01-21)
- [octo-thhx.7] Add data-spotlight attributes to UI elements (closed 2026-01-21)
- [octo-thhx.6] Spotlight overlay component (closed 2026-01-21)
- [octo-thhx.5] octoctl ui CLI commands (closed 2026-01-21)
- [octo-thhx.4] WebSocket ui.* events for agent UI control (closed 2026-01-21)
- [octo-thhx.3] UIControlContext for agent-driven navigation (closed 2026-01-21)
- [octo-ybx2] File viewer toolbar icons colliding/overlapping (closed 2026-01-20)
- [octo-1s4j] Text entered in one chat but not send stays visible when changing chats. this needs to be isolated for each chat and not global across all chats  (closed 2026-01-19)
- [octo-r46b] When viewing one opencode chat I suddenly got the content from another chat rendered. the title was from the actual chat thoug. (closed 2026-01-17)
- [octo-b7sb] When I filter issues, by type (e.g. only bugs) the + button should make this issue type the default in the type dropdown menu of the trx sidebar (closed 2026-01-17)
- [octo-3qja] New agent messages in opencode sessions sometimes only appear after a page reload (closed 2026-01-17)
- [octo-2tcd] Main Chat Architecture Overhaul: Session-Based Conversations (closed 2026-01-17)
- [octo-r25t] login always fails the first time (closed 2026-01-17)
- [workspace-jux6] Improve web app startup performance and reduce reload friction (closed 2026-01-17)
- [octo-8rxq] Fix Main Chat session switching - clicking session in timeline doesn't load messages (closed 2026-01-17)
- [octo-rw4h] "Open in Canvas" doesnt work: No file is rendered to the canvas (closed 2026-01-17)
- [octo-32hw] Fix Main Chat /new button - doesn't clear UI or provide feedback (closed 2026-01-17)
- [octo-sz55] input text does not get removed when sending message sometimes  (closed 2026-01-17)
- [octo-jq8j] Remove/gate client logs that leak auth headers (closed 2026-01-17)
- [octo-ke51] Canvas content is lost on expansions/collapse (closed 2026-01-17)
- [octo-xdyc] Restrict trx commands to validated workspace paths (closed 2026-01-17)
- [octo-e22z] Enforce size limits in write_file (closed 2026-01-17)
- [octo-9x62] Images don't render in main chat and in opencode sessions. placehoder is shown instead. we need to go through the fileserver (closed 2026-01-15)
- [octo-9zek] changes to opencode settings in the sidbar are not getting saved (closed 2026-01-14)
- [octo-g6a4] Opencode chats feel laggy (session navigation + text input) (closed 2026-01-14)
- [octo-kh71] Chat history loads very slowly (closed 2026-01-14)
- [octo-m0bn] issue content from trx doesn't get injected in text input when using "start here" or "start in new session" (closed 2026-01-14)
- [octo-dbr4] text doesn't get removed from input box on send (closed 2026-01-14)
- [octo-8zxp] No gaps between messages in main chat on mobile  (closed 2026-01-14)
- [octo-zh73] Memoize AppShell and SessionsApp components to prevent unnecessary re-renders (closed 2026-01-14)
- [octo-3kwr] Split monolithic AppContext into focused contexts (UIContext, SessionContext, ChatContext) (closed 2026-01-14)
- [octo-51gz] Live model selection is broken (closed 2026-01-14)
- [octo-rhhp] Sessions fail to load with JSON parse error when API returns error response (closed 2026-01-14)
- [octo-qcqj] Auto-scroll to bottom is broken (closed 2026-01-14)
- [octo-jz9c] Scroll-to-bottom button doesn't always appear (closed 2026-01-14)
- [octo-843s] When sending a new message to a suspended chat, the page needs to be reloaded to surface the sent message and the response (closed 2026-01-14)
- [octo-kgph] send messages dissapear and only reappear when agent responds (closed 2026-01-14)
- [octo-4ye3] Radix UI ContextMenu infinite loop on session.updated events (closed 2026-01-14)
- [octo-sf9p] Sidebar collapse button overlaps long titles (closed 2026-01-14)
- [octo-2gvy] 'Resuming session' banner overlaps right sidebar collapse button (desktop) (closed 2026-01-14)
- [octo-cewe] Suspended session banner persists after sending resume message (closed 2026-01-14)
- [octo-54m3] Text input field stays enlarged after sending long message (closed 2026-01-14)
- [octo-rd70] Light theme is too low contrast (closed 2026-01-14)
- [octo-9900] Dictation overlay: live transcript not shown in main textarea (closed 2026-01-14)
- [octo-7m6b] Validate workspace_path against allowed roots (closed 2026-01-14)
- [octo-wjaf] Cap proxy body buffering to avoid memory DoS (closed 2026-01-14)
- [octo-hew1] Add persistent bottom status bar (model, active sessions, version) (closed 2026-01-14)
- [octo-mhdx] Upgrade OpenCode to 1.1.10+ to address CVE-2026-22813 (XSS/RCE vulnerability) (closed 2026-01-14)
- [octo-maa6] Live model selector missing in right settings sidebar (opencode chats) (closed 2026-01-14)
- [octo-5k7c] Sessions disconnect unexpectedly (closed 2026-01-14)
- [octo-5mht] Main chat stream shows only 'working' until tab switch; streaming deltas not rendering (closed 2026-01-14)
- [octo-3m3t] Add /global/event SSE endpoint to backend (closed 2026-01-14)
- [octo-arxh] Update WebSocket handler to use new opencode session routes (closed 2026-01-14)
- [octo-y0en] New messages some times don't render only when user posts follow up or reloads browser  (closed 2026-01-13)
- [octo-21ty] Main Chat streaming replies vanish on view switch (closed 2026-01-13)
- [octo-tf6x] Fix file preview flicker and extend text file support (closed 2026-01-13)
- [octo-twk9] Project templates: new project from templates repo (closed 2026-01-13)
- [octo-gk69] Voice mode TTS stops after a few words (closed 2026-01-13)
- [octo-3df0] Clear file preview on session switch (closed 2026-01-13)
- [octo-1rb4] STT + TSS Improvements (closed 2026-01-13)
- [octo-d1ah] Load ONBOARD.md into main chat session (closed 2026-01-13)
- [octo-pqe0] Main chat canvas expansion does nothing (closed 2026-01-13)
- [octo-6svs] Chat input typing lag (closed 2026-01-13)
- [octo-a8jp] Search result jumps should be instant (closed 2026-01-13)
- [octo-sym3] Agent settings list keys should be unique (closed 2026-01-13)
- [octo-5nzg] Terminal content disappears when expanded (closed 2026-01-13)
- [octo-7a53] Sidebar chat input should stick to bottom when expanded (closed 2026-01-13)
- [octo-3n5p] New session creation should be instant in sidebar (closed 2026-01-13)
- [octo-szp2] Sidebar chat cannot scroll after moving chat (closed 2026-01-13)
- [octo-6193] Chat input hidden when chat moved to sidebar (closed 2026-01-13)
- [octo-z750] Fix expanded view not showing in main panel (closed 2026-01-13)
- [octo-q75j] Fix SessionsApp hook order error (closed 2026-01-13)
- [octo-d8g0] Main chat duplicate message keys cause ordering issues (closed 2026-01-13)
- [octo-stzw] Move chat to sidebar when expanded view is active (closed 2026-01-13)
- [octo-v907] Expand memories/terminal into chat panel (closed 2026-01-13)
- [octo-zk9m] Expand preview/canvas into chat panel and replace file tree (closed 2026-01-13)
- [octo-k8t5] Fix scrollToBottom initialization error (closed 2026-01-13)
- [octo-apmw] Sent chat input reappears after sending (closed 2026-01-13)
- [octo-nb5p] Reduce switch component height (closed 2026-01-13)
- [octo-xc3w] Auto-scroll chat while voice mode is active (closed 2026-01-13)
- [octo-qex7] Add mic mute option in voice mode (closed 2026-01-13)
- [octo-g7n4] Remove rounded corners across UI except radio buttons (closed 2026-01-13)
- [octo-m5p2] Voice mode reads prior messages when activated (closed 2026-01-13)
- [octo-vy54] Read aloud button status flickers during playback (closed 2026-01-13)
- [octo-1gc5] Add integration test for chat image preview URLs (closed 2026-01-13)
- [octo-3sbc] Chat file previews not rendering images (closed 2026-01-13)
- [octo-gqah] Remove mobile chat bottom gap (closed 2026-01-13)
- [octo-g5yw] Mobile chat input background reaches bottom (closed 2026-01-13)
- [octo-t9sq] Allow editing extensionless text files (closed 2026-01-13)
- [octo-5e8a] Inline file preview keeps file tree visible (closed 2026-01-13)
- [octo-ncj2] Cass search agent filter handling (closed 2026-01-13)
- [octo-8wbn] Unify file and preview views (closed 2026-01-13)
- [octo-py9j] Icon-only send button styling (closed 2026-01-13)
- [octo-ghdb] Mobile chat input bar fills width (closed 2026-01-13)
- [octo-5de7] Cass search: mobile mode switch + results (closed 2026-01-13)
- [workspace-hg9w] Beads dashboard in Projects View (closed 2026-01-12)
- [workspace-o6a7] Main Chat: mmry integration (closed 2026-01-12)
- [octo-vmpt] Main Chat: Pi agent runtime integration (closed 2026-01-12)
- [octo-knwd] Main Chat: Integrate Pi agent runtime (closed 2026-01-12)
- [octo-wmrf] A2UI Integration - Agent-Driven UI for Octo (closed 2026-01-12)
- [octo-p3yr] test (closed 2026-01-12)
- [octo-mwhw] Test issue (closed 2026-01-12)
- [octo-n2xy] Canvas: Canvas content disappering when switching tabs (closed 2026-01-12)
- [octo-n2xy.1] Canvas state loss needs proper solution (state lifting or context) (closed 2026-01-12)
- [octo-rmt7] Tauri app: bottom input area has incorrect background color (closed 2026-01-11)
- [octo-d4e5] right clicking always also selects parts of the interface as text on mobile (e.g. when trying to just open a context menu, part of the interface is also always selected) (closed 2026-01-11)
- [octo-ga3h] Tauri: File access permissions for Tauri app not set (closed 2026-01-11)
- [workspace-92h2] Context window gauge doesn't reset after compaction - need to monitor opencode events and reset (closed 2026-01-11)
- [octo-zmvc] Main chat mmry on frontend not displaying the correct store (closed 2026-01-11)
- [octo-phy4] Pasted images only have "Image.png" as the name, even when pasting multiple images. we need to enumerate better (closed 2026-01-11)
- [octo-wmrf.6] CLI A2UI Renderer (closed 2026-01-11)
- [octo-wmrf.2] Backend A2UI Request Manager (closed 2026-01-11)
- [octo-wmrf.4] Integrate A2UI in Chat Timeline (closed 2026-01-11)
- [octo-wmrf.3] React A2UI Renderer (closed 2026-01-11)
- [workspace-vmu2] Main Chat: Persistent cross-project AI assistant (closed 2026-01-11)
- [octo-ww2y] Enforce Bearer auth on backend proxies (closed 2026-01-11)
- [octo-wmrf.1] A2UI Message Part Type (closed 2026-01-11)
- [octo-wmrf.9] Backend Test Harness for Mock Messages (closed 2026-01-11)
- [octo-wmrf.8] Disable OpenCode mcp_question tool (closed 2026-01-11)
- [octo-n13x] Make the entire interface perfectly navigable via keyboard (closed 2026-01-11)
- [workspace-5pmk] Tauri Desktop & Mobile App (closed 2026-01-11)
- [workspace-gg16] Integrate mmry memory system into Octo frontend (closed 2026-01-11)
- [octo-7yrq] Tauri iOS: Native HTTP via reqwest for reliable networking (closed 2026-01-09)
- [octo-zhzr] Add model switcher + fix opencode permission/error events (closed 2026-01-09)
- [octo-7yjt] Build octo-runner daemon for multi-user process isolation (closed 2026-01-07)
- [octo-gfsk] Main Chat: Personality templates (PERSONALITY.md, USER.md, enhanced AGENTS.md) (closed 2026-01-06)
- [workspace-jux6.2] Cache auth status to avoid repeated /api/me calls (closed 2026-01-06)
- [octo-bvw0] Adopt single opencode per user with directory scoping + per-dir fileserver view (closed 2026-01-06)
- [octo-2qa6] Dictation gets progressively slower while speaking - buffering issue (closed 2026-01-06)
- [octo-xf08] Image annotation canvas in middle panel (closed 2026-01-05)
- [octo-v2ed] TRX sidebar integration - view and edit issues in right panel (closed 2026-01-05)
- [octo-v2ed.1] Backend: Add trx proxy API routes (closed 2026-01-05)
- [octo-2h79] Session reliability fixes (proxy resume, local EAVS, startup retry) (closed 2026-01-05)
- [workspace-8t16] mmry proxy in single-user mode doesn't pass store parameter - all sessions show same memories (closed 2026-01-04)
- [workspace-pnwb] Main Chat: History injection on session start (closed 2026-01-04)
- [workspace-u73l] Main Chat: Frontend session threading (closed 2026-01-04)
- [workspace-2hjz] Remove preload screen after mount (closed 2026-01-04)
- [workspace-1fqu] Improve Vite dev startup and message load responsiveness (closed 2026-01-04)
- [workspace-rr3j] Main Chat: First-start setup flow (closed 2026-01-04)
- [workspace-oewk] Main Chat: opencode plugin for compaction (closed 2026-01-04)
- [workspace-jj70] Add resume button when disconnected (closed 2026-01-04)
- [workspace-c42p] Main Chat: Backend API endpoints (closed 2026-01-04)
- [workspace-8ink] Main Chat: Per-assistant SQLite database (closed 2026-01-04)
- [workspace-h9pr] Fix local session resume to reallocate ports (closed 2026-01-04)
- [workspace-paxf] Auto-attach running sessions + improve reconnect handling (closed 2026-01-04)
- [workspace-yiq1] Eliminate backend warnings (closed 2026-01-04)
- [workspace-30r0] Migrate frontend from Next.js to Vite + React (closed 2026-01-03)
- [workspace-30r0.6] Remove Next.js and clean up dependencies (closed 2026-01-03)
- [workspace-30r0.5] Update build and dev scripts (closed 2026-01-03)
- [workspace-30r0.4] Migrate components and hooks (closed 2026-01-03)
- [workspace-30r0.3] Replace next-intl with react-i18next (closed 2026-01-03)
- [workspace-30r0.2] Set up routing with React Router or TanStack Router (closed 2026-01-03)
- [workspace-30r0.1] Initialize Vite + React project structure (closed 2026-01-03)
- [workspace-6tr3] Dictation should work while focused in input box (closed 2026-01-03)
- [workspace-3o61] Fix sidebar icon buttons to be circles properly (closed 2026-01-03)
- [workspace-gg16.8] Config: Add mmry settings to octo config.toml (closed 2026-01-03)
- [workspace-ts8y] Settings system: Schema-driven config management UI (closed 2026-01-03)
- [workspace-ts8y.6] Frontend: Settings navigation and command palette (closed 2026-01-03)
- [workspace-ts8y.5] Frontend: SettingsEditor component (closed 2026-01-03)
- [workspace-ts8y.4] Backend: Wire up settings services in main.rs (closed 2026-01-03)
- [workspace-ts8y.3] Schema: Update config schema with x-scope and voice settings (closed 2026-01-03)
- [workspace-ts8y.2] Backend: Settings API endpoints (closed 2026-01-03)
- [workspace-ts8y.1] Backend: Settings module with schema filtering and TOML management (closed 2026-01-03)
- [workspace-fz04] Dictation button for speech-to-text input (closed 2026-01-03)
- [workspace-u4yp] Project filter bar in chat sidebar with circular icons (closed 2026-01-03)
- [workspace-mnwx] Project logos: Auto-detect and display logos from logo/ directory (closed 2026-01-03)
- [workspace-nbjc] Voice mode continues playing after stop button clicked (closed 2026-01-03)
- [workspace-bam5] Add slash commands with popup list and fuzzy filter (closed 2026-01-02)
- [workspace-98bw] Evaluate rootless Podman as default container runtime (closed 2026-01-02)
- [workspace-y8em] Memory delete button not visible on mobile (closed 2026-01-02)
- [workspace-j2qv] mmry search returns 'Not Found' error in frontend (closed 2026-01-02)
- [workspace-9ekt] Save button frozen when editing files (closed 2026-01-01)
- [workspace-nkl1] EavsClient::new panics on HTTP client build failure (closed 2025-12-31)
- [workspace-zjat] Unused syntax highlighter imports in PreviewView increase bundle size (closed 2025-12-31)
- [workspace-b5yr] Duplicate projectDefaultAgents localStorage state and event handlers (closed 2025-12-31)
- [workspace-hj2w] fileserver upload buffers entire file in memory (closed 2025-12-31)
- [workspace-gfnq] fileserver upload can follow symlink and write outside root (closed 2025-12-31)
- [workspace-d7rq] Auth tokens stored in localStorage and passed via URL query params (closed 2025-12-31)
- [workspace-vz0r] Frontend auto-login uses hardcoded dev credentials (closed 2025-12-31)
- [workspace-w2vi] AuthConfig defaults to dev_mode with known dev credentials (closed 2025-12-31)
- [workspace-8vv8] Context Window Gauge: Dynamic model limits from models.dev (closed 2025-12-31)
- [workspace-gg16.1] Host: Configure central mmry-service for embeddings (closed 2025-12-31)
- [workspace-gg16.6] Container mode: Add mmry to agent container image (closed 2025-12-31)
- [workspace-vhvq] Show busy agents in chat history sidebar (closed 2025-12-30)
- [workspace-twx2] Fix sent message reappearing in text input (closed 2025-12-30)
- [workspace-eo1i] Fix message flickering during polling updates (closed 2025-12-30)
- [workspace-wtbk] Remove bottom stop button from chat input (closed 2025-12-30)
- [workspace-4d69] Add activity indicator for busy sessions in sidebar (closed 2025-12-30)
- [workspace-wgv5] Add context window gauge to chat UI (closed 2025-12-30)
- [workspace-onrt.2] Auto Linux user creation implemented (closed 2025-12-30)
- [workspace-onrt.1] AgentRPC API handlers and tests (closed 2025-12-30)
- [workspace-onrt] Multi-user architecture: AgentRPC abstraction (closed 2025-12-30)
- [workspace-5t8k] Add backend mode configuration (closed 2025-12-30)
- [workspace-5gpd] Standardize data storage location across modes (closed 2025-12-30)
- [workspace-mswd] Refactor Docker backend to use AgentRPC interface (closed 2025-12-30)
- [workspace-9cjp] Implement Local backend with systemd user services (closed 2025-12-30)
- [workspace-ahuw] Define AgentRPC interface for unified local/docker backends (closed 2025-12-30)
- [workspace-bd08] Lazy startup of opencode on message send (closed 2025-12-30)
- [workspace-dnv0] Session lifecycle: idle timeout and LRU cleanup (closed 2025-12-30)
- [workspace-wmr3] Multi-workspace session resume: start opencode for history session's workspace (closed 2025-12-30)
- [workspace-ut8u] Load chat messages from disk for viewing history (closed 2025-12-30)
- [workspace-idug] Fix project session count mismatch (closed 2025-12-30)
- [workspace-2oao] Remove 100 session limit from chat history (closed 2025-12-30)
- [workspace-y074] Agent/Persona Management UI (closed 2025-12-29)
- [workspace-kdxm] persona.toml structure for agent + UI metadata (closed 2025-12-29)
- [workspace-wslo] Workdir picker when starting chat (closed 2025-12-29)
- [workspace-cfg0] Meta-agent sidebar for agent building (closed 2025-12-29)
- [workspace-9fiq] Agent creator UI with form fields (closed 2025-12-29)
- [workspace-bp30] Agents view: list and manage opencode agents (closed 2025-12-29)
- [workspace-9wu5] Projects view: list workspace directories (closed 2025-12-29)
- [workspace-2jb2] Add project name to chat metadata row (closed 2025-12-29)
- [workspace-nsg5] Refactor sidebar: Chats/Projects/Agents tabs (closed 2025-12-29)
- [workspace-mrri] Refactor Persona to reference opencode agents (closed 2025-12-29)
- [workspace-aasa] Per-chat agent working indicator (closed 2025-12-29)
- [workspace-ka70] Per-chat draft text persistence (closed 2025-12-29)
- [workspace-lkqj] Fix chat pinning functionality (closed 2025-12-29)
- [workspace-u35] Security: Per-user UID isolation for container mounts (closed 2025-12-29)
- [workspace-92j] Idiomatic: let _ = user pattern to silence unused variable - use underscore prefix instead (closed 2025-12-29)
- [workspace-8qs] Idiomatic: Excessive use of clone() on Arc - Arc::clone(&x) preferred for clarity (closed 2025-12-29)
- [workspace-ulx] LOW: Redundant canonicalization in fileserver (closed 2025-12-29)
- [workspace-eah] LOW: Magic numbers without constants (closed 2025-12-29)
- [workspace-cbm] LOW: Verbose pattern match could use if-let (closed 2025-12-29)
- [workspace-7dk] LOW: Unnecessary lifetime parameter (closed 2025-12-29)
- [workspace-jgh] LOW: Manual Default impl could use derive (closed 2025-12-29)
- [workspace-n92] LOW: Unused import warning suppression (closed 2025-12-29)
- [workspace-9rz] LOW: Inefficient string allocation for extension checks (closed 2025-12-29)
- [workspace-rg4] LOW: Repeated modified time extraction pattern (closed 2025-12-29)
- [workspace-fv3] LOW: Repeated SuccessResponse construction (closed 2025-12-29)
- [workspace-4vj] LOW: Using Box<dyn Error> instead of anyhow in main (closed 2025-12-29)
- [workspace-677] LOW: Manual error conversion instead of From impl (closed 2025-12-29)
- [workspace-czk] LOW: Using to_string_lossy without handling invalid UTF-8 (closed 2025-12-29)
- [workspace-oux] LOW: Blocking I/O in Config::from_file (closed 2025-12-29)
- [workspace-j0q] Performance: ContainerRuntime clones the entire runtime for each session service operation (closed 2025-12-29)
- [workspace-y9p] Performance: Invite code batch creation runs N sequential DB queries instead of batch insert (closed 2025-12-29)
- [workspace-rta] Code Duplication: Cookie building logic duplicated across login, dev_login, and register handlers (closed 2025-12-29)
- [workspace-9to] Code Duplication: SQL SELECT column list repeated in 6+ repository methods without a const or macro (closed 2025-12-29)
- [workspace-kh1] Code Duplication: Error handling pattern for container commands repeated in every ContainerRuntime method (closed 2025-12-29)
- [workspace-qlu] Idiomatic: Use thiserror for all error types instead of manual impl for ConfigValidationError (closed 2025-12-29)
- [workspace-do4] Idiomatic: Use From trait instead of into() calls for type conversions where possible (closed 2025-12-29)
- [workspace-d89] Idiomatic: Replace manual string building with format! or write! macros in container command args (closed 2025-12-29)
- [workspace-rys] Idiomatic: Use ? operator consistently instead of explicit match on Result in handlers (closed 2025-12-29)
- [workspace-w1x] Idiomatic: Use Cow<str> for paths that may or may not be owned to reduce allocations (closed 2025-12-29)
- [workspace-4yi] Correctness: Delete session does not check if container still exists before returning success (closed 2025-12-29)
- [workspace-u2y] MEDIUM: Blocking I/O in async context (fileserver) (closed 2025-12-29)
- [workspace-7pu] MEDIUM: Repeated root path validation pattern (closed 2025-12-29)
- [workspace-6b5] MEDIUM: Silent error ignoring in fileserver WalkDir (closed 2025-12-29)
- [workspace-vqp.8] EAVS logging: Stream logs to backend for monitoring/analytics (closed 2025-12-29)
- [workspace-vqp] Integrate EAVS for AI provider interception and virtual keys (closed 2025-12-29)
- [workspace-vqp.6] Backend: Aggregate usage/cost data from EAVS per session/user (closed 2025-12-29)
- [workspace-6f9] Test SSE streaming in production environment (closed 2025-12-29)
- [workspace-c7l] Configure Caddy for SSE proxy passthrough (closed 2025-12-29)
- [workspace-ivc] Architecture: Support multiple opencode agents per container (closed 2025-12-29)
- [workspace-ot3] Architecture: Multi-user project collaboration via additional containers (closed 2025-12-29)
- [workspace-hcl] Add list virtualization for chat messages in sessions app (closed 2025-12-29)
- [workspace-2w2] User modes: Simplified, Light Coding, Terminal Terminator (closed 2025-12-29)
- [workspace-p54] display user mode after first login (closed 2025-12-29)
- [workspace-ivc.15] Backend: Track agent per opencode session (closed 2025-12-29)
- [workspace-xba] Frontend: Model switcher component (closed 2025-12-29)
- [workspace-4k1] Backend: Add user_mode to user profile (closed 2025-12-29)
- [workspace-zl6] Security: Logout does not include Secure flag conditional like login does (closed 2025-12-29)
- [workspace-mz4] Security: Port range finder has no upper bound check before 65530 allowing very high port allocation (closed 2025-12-29)
- [workspace-e4c] Security: proxy_request forwards all headers including potentially sensitive ones to containers (closed 2025-12-29)
- [workspace-99m] Security: File upload loads entire file into memory before size check (closed 2025-12-29)
- [workspace-2qd] Code Duplication: UserInfo struct duplicated in handlers.rs vs user/models.rs with different field names (closed 2025-12-29)
- [workspace-dqo] Idiomatic: Replace Vec<String> bind_values pattern with query builder or typed query approach (closed 2025-12-29)
- [workspace-eje] Correctness: Race condition between port allocation query and container creation (closed 2025-12-29)
- [workspace-483] Correctness: WebSocket proxy silently drops Ping/Pong messages instead of forwarding them (closed 2025-12-29)
- [workspace-85i] Correctness: opencode_events SSE handler is stub that only sends keepalives, not actual events (closed 2025-12-29)
- [workspace-1c1] Correctness: get_or_create_from_oidc does not actually link external_id to existing user (closed 2025-12-29)
- [workspace-w9x] Correctness: Session 2-second sleep before marking running is arbitrary and may not reflect actual readiness (closed 2025-12-29)
- [workspace-gok] Correctness: stop_session does not wait for container stop before marking stopped in DB (closed 2025-12-29)
- [workspace-415] Correctness: Unused code flagged by compiler warnings should be cleaned up or feature-gated (closed 2025-12-29)
- [workspace-97k] Implement proper SSE client with reconnection logic (closed 2025-12-29)
- [workspace-2az] Terminal WebSocket connection is slow (5-10 second delay) (closed 2025-12-29)
- [workspace-me0] opencode session names are still not the generated human readable names in the frontend. Its always called 'Workspace Session' but each session has a unique specific descriptive name (closed 2025-12-29)
- [workspace-917] Add horizontal margins inside terminal content (closed 2025-12-29)
- [workspace-vkd] pinning a chat in the sidebar does nothing (closed 2025-12-29)
- [workspace-ivc.10] Frontend: Agent display and filtering in session sidebar (closed 2025-12-29)
- [workspace-ivc.11] Frontend: Overhaul Agents section (closed 2025-12-29)
- [workspace-9uvd] Show persona in chat history list (closed 2025-12-29)
- [workspace-qu71] Create persona detail/edit view (closed 2025-12-29)
- [workspace-qvx] Implement dual-mode file browser UI (simple vs power user) (closed 2025-12-29)
- [workspace-e6k] Implement user file injection via @filename references (closed 2025-12-29)
- [workspace-r9o] MEDIUM: File size checked after full read (DoS vector) (closed 2025-12-29)
- [workspace-qsz] MEDIUM: Overly permissive CORS in fileserver (closed 2025-12-29)
- [workspace-wyv] MEDIUM: Unnecessary clone of Session in spawned task (closed 2025-12-29)
- [workspace-6ue] MEDIUM: String-based error type matching is fragile (closed 2025-12-29)
- [workspace-a8n] MEDIUM: Duplicate proxy error handling (closed 2025-12-29)
- [workspace-7rz] MEDIUM: Duplicate user query boilerplate (closed 2025-12-29)
- [workspace-v0y] MEDIUM: Duplicate session query boilerplate (closed 2025-12-29)
- [workspace-7nb] MEDIUM: Database pool size may be too small (closed 2025-12-29)
- [workspace-6hl] MEDIUM: Blocking filesystem check in async context (closed 2025-12-29)
- [workspace-72t] MEDIUM: No input validation on session workspace path (closed 2025-12-29)
- [workspace-26u] MEDIUM: No rate limiting on login endpoint (closed 2025-12-29)
- [workspace-lfu.22] App Documentation Template (closed 2025-12-29)
- [workspace-lfu.20] Data Table Component (closed 2025-12-29)
- [workspace-lfu.19] Metrics Cards Component (closed 2025-12-29)
- [workspace-lfu.17] Keyboard Shortcuts System (closed 2025-12-29)
- [workspace-lfu.14] Status Bar Component (closed 2025-12-29)
- [workspace-9aa.4.7] Add byteowlz toolbox to container image (closed 2025-12-29)
- [workspace-9aa.4.6] Implement template generation system in backend (closed 2025-12-29)
- [workspace-9aa.3.9] Implement session view - code editor panel (closed 2025-12-29)
- [workspace-9aa.3.2] Implement OIDC authentication with dev bypass (closed 2025-12-29)
- [workspace-9aa] AI Agent Workspace Platform - V1 Implementation (closed 2025-12-29)
- [workspace-9aa.4] Agent Runtime & Templates (closed 2025-12-29)
- [workspace-9aa.3] Frontend Application (closed 2025-12-29)
- [workspace-9aa.2] Backend Control Plane (closed 2025-12-29)
- [workspace-9aa.1.4] Set up GitLab CI/CD pipeline (closed 2025-12-29)
- [workspace-9aa.1.2] Create Ansible playbook for application deployment (closed 2025-12-29)
- [workspace-9aa.1.1] Create Ansible playbook for VPS base setup (closed 2025-12-29)
- [workspace-9aa.1] Infrastructure & Deployment (closed 2025-12-29)
- [workspace-6la] Scheduled tasks / Cron system (closed 2025-12-29)
- [workspace-0w6] Integrate byt for project scaffolding (closed 2025-12-29)
- [workspace-7mz] Integrate mmry for agent memory (closed 2025-12-29)
- [workspace-tb6] Integrate dgrm (Excalidraw) for diagramming (closed 2025-12-29)
- [workspace-6ot] Integrate A2UI (Artifact-to-UI) support (closed 2025-12-29)
- [workspace-ivg] Live voice mode (closed 2025-12-29)
- [workspace-dk5] Fix skill locations to match opencode discovery paths (closed 2025-12-29)
- [workspace-s02] Config: Add agent.workdir to config.toml (closed 2025-12-29)
- [workspace-45p] Add WebSocket support for real-time events (closed 2025-12-29)
- [workspace-c1h.3] Persona system: Container injection (closed 2025-12-29)
- [workspace-c1h] Persona system: Core architecture (closed 2025-12-29)
- [workspace-c1h.2] Persona system: Frontend picker UI (closed 2025-12-29)
- [workspace-c1h.1] Persona system: Backend API (closed 2025-12-29)
- [workspace-u7j] Backend: Add readiness checks before marking session running (closed 2025-12-29)
- [workspace-l4n] Frontend: Add session management (stop/delete/upgrade) (closed 2025-12-29)
- [workspace-ltxe] Frontend: Use server-side syntax highlighting in PreviewView (closed 2025-12-28)
- [workspace-x74g] Server-side syntax highlighting in fileserver (closed 2025-12-28)
- [workspace-01x] Show persona indicator in chat view (closed 2025-12-28)
- [workspace-0qc] Replace Agents section with Personas in frontend (closed 2025-12-28)
- [workspace-zln] Add persona info to session/chat model (closed 2025-12-28)
- [workspace-acx] Define default persona (govnr) (closed 2025-12-28)
- [workspace-npk] Investigate: x-opencode-directory behavior with sessions (closed 2025-12-28)
- [workspace-34b] URGENT: Fix live permission display on frontend (closed 2025-12-28)
- [workspace-egj] Mobile UI optimizations: native app feel (closed 2025-12-27)
- [workspace-upf] Agent runtime status + scaffolding integration (byt) (closed 2025-12-26)
- [workspace-9aa.2.11] Implement admin observability endpoints (closed 2025-12-25)
- [workspace-wrv] Add WebSocket endpoint for file change notifications (closed 2025-12-25)
- [workspace-ivc.16] Backend: Create agent directory and AGENTS.md (closed 2025-12-18)
- [workspace-ivc.9] Backend proxy routing for sub-agents (closed 2025-12-18)
- [workspace-ivc.13] Control plane: Agent management API (closed 2025-12-18)
- [workspace-ivc.6] Refactor container lifecycle: stop without remove (closed 2025-12-18)
- [workspace-ivc.8] Add agent discovery endpoint to fileserver or backend (closed 2025-12-18)
- [workspace-ivc.14] Container: Agent helper script for control plane (closed 2025-12-18)
- [workspace-ivc.12] Hot-reload agents when new directory is created (closed 2025-12-18)
- [workspace-ivc.7] Update entrypoint.sh to start multiple agents (closed 2025-12-18)
- [workspace-ivc.4] Frontend: Multi-agent session view (closed 2025-12-18)
- [workspace-ivc.5] Proxy routing for sub-agents (closed 2025-12-18)
- [workspace-ivc.3] Backend API: Register and track sub-agents (closed 2025-12-18)
- [workspace-ivc.2] Agent directory structure convention (closed 2025-12-18)
- [workspace-ivc.1] Agent spawner service inside container (closed 2025-12-18)
- [workspace-cjs] Fix Dockerfile.arm64 parity + remove chsh (closed 2025-12-18)
- [workspace-nk8] Container: Mount workspace as user home directory (closed 2025-12-18)
- [workspace-ycc] LOW: Repeated string allocations in error mapping (closed 2025-12-18)
- [workspace-16p] Security: EAVS virtual key stored temporarily in database before being cleared (closed 2025-12-18)
- [workspace-770] Correctness: Session container startup spawned in background with no way to await or check result (closed 2025-12-18)
- [workspace-cw5] LOW: Minimum password length too short (6 chars) (closed 2025-12-18)
- [workspace-gfo] Fix podman JSON parsing for Created field returning integer instead of string (closed 2025-12-17)
- [workspace-1hu] Orchestrator: Handle orphan containers and port conflicts (closed 2025-12-17)
- [workspace-jah] Add download and ZIP endpoints to fileserver (closed 2025-12-17)
- [workspace-ur0] Add file upload, download, and multi-select to FileTreeView (closed 2025-12-17)
- [workspace-f7j] Enhance FileTreeView with collapsible folders, navigation, and state persistence (closed 2025-12-17)
- [workspace-iqa] Reorder sidebar navigation - Chats first (closed 2025-12-17)
- [workspace-39d] Update sidebar logo to new OCTO branding with theme support (closed 2025-12-17)
- [workspace-lzt] Command palette added to frontend (closed 2025-12-17)
- [workspace-37i] Deleting a chat from the context menu in the sidebar doesnt work (closed 2025-12-17)
- [workspace-605] Terminal shows 'Waiting...' even when connected (closed 2025-12-17)
- [workspace-s4q] Add client-side message caching to reduce API calls (closed 2025-12-17)
- [workspace-r0a] Fix ghostty-web terminal memory leak - clean up sessionConnections map entries (closed 2025-12-17)
- [workspace-mot] Throttle ResizeObserver in ghostty-terminal to prevent excessive fit() calls (closed 2025-12-17)
- [workspace-eeg] Memoize markdown-renderer components to prevent expensive re-renders (closed 2025-12-17)
- [workspace-bb6] Fix KnightRiderSpinner 60fps animation - use CSS animations or requestAnimationFrame (closed 2025-12-17)
- [workspace-vvt] Optimize message polling frequency (closed 2025-12-17)
- [workspace-63u] Consolidate polling intervals - replace multiple polling with single SSE connection (closed 2025-12-17)
- [workspace-1om] Performance: build_tree uses sync std::fs::read_dir in async context (closed 2025-12-17)
- [workspace-9ak] MEDIUM: HTTP client created per request (performance) (closed 2025-12-17)
- [workspace-waa] Performance: HTTP client created per-request in proxy_request instead of being reused (closed 2025-12-17)
- [workspace-5yn] Performance: bd commands take ~10s due to daemon auto-start (closed 2025-12-17)
- [workspace-0mn] Frontend: Add new chat button to sidebar (closed 2025-12-17)
- [workspace-p2e] Fix startup-time proxy flakiness (502/WS) (closed 2025-12-17)
- [workspace-rrl] Frontend: stop 300ms /session/status polling (use SSE) (closed 2025-12-17)
- [workspace-ht4] Terminal WebSocket connects but immediately closes - stuck on connecting (closed 2025-12-17)
- [workspace-mzz] Frontend: Fix missing logo asset causing Next image 400 (closed 2025-12-17)
- [workspace-86w] Fix React Strict Mode double-invoke breaking WebSocket connections in GhosttyTerminal (closed 2025-12-17)
- [workspace-8pn] Security: Container runtime uses shell command execution without sanitizing image names (closed 2025-12-16)
- [workspace-crz] Correctness: register handler validates invite then creates user then consumes - TOCTOU vulnerability (closed 2025-12-16)
- [workspace-lc8] Security: DevUser bcrypt hashing at default() runs on startup, causing slowdown and leaking timing (closed 2025-12-16)
- [workspace-eq9] Security: Weak JWT secret generation uses non-cryptographic PRNG (closed 2025-12-16)
- [workspace-vqp.5] Backend API: Revoke virtual key when session ends (closed 2025-12-16)
- [workspace-vqp.4] Backend API: Set budget/rate limits per user on virtual keys (closed 2025-12-16)
- [workspace-vqp.3] Backend API: Create virtual key when session starts (closed 2025-12-16)
- [workspace-vqp.1] Add EAVS to container image (install/configure in Dockerfile) (closed 2025-12-16)
- [workspace-vqp.7] EAVS config: Configure upstream providers (env vars for API keys) (closed 2025-12-16)
- [workspace-vqp.2] Configure opencode to use EAVS as proxy endpoint (closed 2025-12-16)
- [workspace-7va] Refactor podman module to support both Docker and Podman runtimes (closed 2025-12-16)
- [workspace-7va.5] Update compose file comments and documentation (closed 2025-12-16)
- [workspace-7va.4] Update backend config.toml with container runtime setting (closed 2025-12-16)
- [workspace-7va.3] Handle Docker vs Podman differences (volume :Z suffix, etc) (closed 2025-12-16)
- [workspace-7va.2] Add ContainerRuntime enum (Docker/Podman) and config option (closed 2025-12-16)
- [workspace-7va.1] Rename podman module to container module (closed 2025-12-16)
- [workspace-vj3] Implement invite code authentication system (closed 2025-12-16)
- [workspace-vj3.8] Update frontend login to use JWT auth (closed 2025-12-16)
- [workspace-vj3.7] Build frontend registration page (closed 2025-12-16)
- [workspace-vj3.6] Implement CLI for invite code batch generation (closed 2025-12-16)
- [workspace-vj3.5] Add admin API for invite code management (closed 2025-12-16)
- [workspace-vj3.4] Implement JWT authentication flow (closed 2025-12-16)
- [workspace-vj3.3] Add registration endpoint with invite code validation (closed 2025-12-16)
- [workspace-vj3.2] Implement invite code repository and service (closed 2025-12-16)
- [workspace-vj3.1] Add invite_codes table to database schema (closed 2025-12-16)
- [workspace-9aa.4.5] Create Meeting Synth template configuration (closed 2025-12-16)
- [workspace-9aa.4.4] Create Research Assistant template configuration (closed 2025-12-16)
- [workspace-9aa.4.3] Create Coding Copilot template configuration (closed 2025-12-16)
- [workspace-lfu.21] Real-time Charts Component (closed 2025-12-16)
- [workspace-lfu.12] Preview Panel Component (closed 2025-12-16)
- [workspace-lfu.18] Empty States & Loading Patterns (closed 2025-12-16)
- [workspace-lfu.15] Toast/Notification System (closed 2025-12-16)
- [workspace-35m] Add i18n (internationalization) support to frontend (closed 2025-12-16)
- [workspace-9aa.2.12] Add structured logging and error handling (closed 2025-12-16)
- [workspace-9aa.2.10] Implement session management API (closed 2025-12-16)
- [workspace-9aa.2.9] Implement user and project management API (closed 2025-12-16)
- [workspace-9aa.2.7] Implement WebSocket proxy for terminal (closed 2025-12-16)
- [workspace-9aa.2.6] Implement HTTP proxy for opencode API (closed 2025-12-16)
- [workspace-9aa.1.3] Configure Caddy reverse proxy (closed 2025-12-16)
- [workspace-9aa.1.5] Create docker-compose/podman-compose for local development (closed 2025-12-16)
- [workspace-vrc] HIGH: Repeated error-to-ApiError conversion logic (duplication) (closed 2025-12-16)
- [workspace-btb] HIGH: Full file read into memory for downloads (closed 2025-12-16)
- [workspace-qzc] HIGH: Memory leak via Box::leak in podman module (closed 2025-12-16)
- [workspace-037] MEDIUM: Directory deletion allows deleting root (closed 2025-12-16)
- [workspace-q05] HIGH: Unsanitized uploaded filename in fileserver (closed 2025-12-16)
- [workspace-wix] HIGH: Path traversal TOCTOU vulnerability in fileserver (closed 2025-12-16)
- [workspace-96q] HIGH: CORS allows any origin in backend API (closed 2025-12-16)
- [workspace-dku] HIGH: Auth cookie missing Secure flag (closed 2025-12-16)
- [workspace-mbz] CRITICAL: Plaintext passwords stored for dev users (closed 2025-12-16)
- [workspace-wss] CRITICAL: Hardcoded default JWT secret in auth config (closed 2025-12-16)
- [workspace-11i] Implement file preview components in frontend (closed 2025-12-15)
- [workspace-9aa.3.15] Implement admin dashboard - metrics charts (closed 2025-12-15)
- [workspace-9aa.3.14] Implement admin dashboard - sessions list (closed 2025-12-15)
- [workspace-9aa.3.13] Implement admin dashboard - overview (closed 2025-12-15)
- [workspace-9aa.3.12] Implement resizable split-pane layout (closed 2025-12-15)
- [workspace-9aa.3.11] Implement session view - preview panel (closed 2025-12-15)
- [workspace-9aa.3.10] Implement session view - terminal panel (closed 2025-12-15)
- [workspace-9aa.3.8] Implement session view - file tree panel (closed 2025-12-15)
- [workspace-9aa.3.7] Implement session view - chat panel (closed 2025-12-15)
- [workspace-9aa.3.6] Implement agent gallery/selector (closed 2025-12-15)
- [workspace-9aa.3.5] Implement workspace picker page (closed 2025-12-15)
- [workspace-9aa.3.4] Create shared layout with navigation (closed 2025-12-15)
- [workspace-9aa.3.3] Set up internationalization (i18n) with next-intl (closed 2025-12-15)
- [workspace-bxo] Integrate fileserver API into frontend file browser (closed 2025-12-15)
- [workspace-9oq] Route fileserver requests through backend proxy with auth (closed 2025-12-15)
- [workspace-9aa.4.2] Create entrypoint script for container (closed 2025-12-15)
- [workspace-8hj] Add fileserver binary to container build pipeline (closed 2025-12-15)
- [workspace-9aa.4.1] Create base agent-runtime Containerfile (closed 2025-12-15)
- [workspace-9aa.2.5] Implement session orchestration service (closed 2025-12-15)
- [workspace-9aa.3.1] Set up Next.js project structure and routing (closed 2025-12-15)
- [workspace-lfu.16] App State Management Pattern (closed 2025-12-15)
- [workspace-lfu.13] Resizable Split Pane System (closed 2025-12-15)
- [workspace-lfu.8] Agent Template Selector - Clean Card UI (closed 2025-12-15)
- [workspace-lfu.7] Admin App - Dashboard & Monitoring (closed 2025-12-15)
- [workspace-lfu.6] Sessions App - Active Session Interface (closed 2025-12-15)
- [workspace-lfu.5] Workspaces App - Grid View & Actions (closed 2025-12-15)
- [workspace-lfu.11] Terminal Component - xterm.js Integration (closed 2025-12-15)
- [workspace-lfu.10] File Tree Component (closed 2025-12-15)
- [workspace-lfu.9] Chat Interface Component (closed 2025-12-15)
- [workspace-9aa.3.16] Create API client and React Query setup (closed 2025-12-15)
- [workspace-lfu.4] Navigation System - Text-Based Menu Items (closed 2025-12-15)
- [workspace-lfu.3] App Shell Layout - Sidebar & Main Content Area (closed 2025-12-15)
- [workspace-lfu.2] App Registry System - Core Architecture (closed 2025-12-15)
- [workspace-9aa.2.4] Implement Podman container management (closed 2025-12-15)
- [workspace-9aa.2.2] Implement SQLite database layer with sqlx (closed 2025-12-15)
- [workspace-9aa.2.1] Refactor backend from CLI to Axum web server (closed 2025-12-15)
- [workspace-ook] Implement standalone Rust fileserver for container workdir access (closed 2025-12-15)
- [workspace-8zn] Source map error in ghostty-web WebAssembly (closed 2025-12-15)
- [workspace-er5] React hydration mismatch due to Dark Reader browser extension (closed 2025-12-15)
- [workspace-unw] Layout forced before page fully loaded causing flash of unstyled content (closed 2025-12-15)
- [workspace-5h0.7] Update frontend to use backend API for session management (closed 2025-12-15)
- [workspace-ztq] Terminal WebSocket connection failure to localhost:41822 (closed 2025-12-15)
- [workspace-9xw] SSE connection failure to opencode event API endpoint (closed 2025-12-15)
- [workspace-cyl] API /api/opencode/event returning Internal Server Error (closed 2025-12-15)
- [workspace-9aa.2.3] Implement storage abstraction layer (closed 2025-12-15)
- [workspace-9aa.2.8] Implement authentication middleware (closed 2025-12-15)
- [workspace-5h0] Implement Rust backend container orchestration via Podman (closed 2025-12-15)
- [workspace-5h0.6] Add serve subcommand to start the API server (closed 2025-12-15)
- [workspace-5h0.5] Implement REST API endpoints for session lifecycle (closed 2025-12-15)
- [workspace-5h0.4] Implement HTTP/WebSocket proxy for container services (closed 2025-12-15)
- [workspace-5h0.3] Implement session management and state tracking (closed 2025-12-15)
- [workspace-5h0.2] Implement Podman container management module (closed 2025-12-15)
- [workspace-5h0.1] Add Axum web framework and core dependencies (closed 2025-12-15)
- [workspace-tcz] Multi-container architecture with Traefik routing (closed 2025-12-12)
- [workspace-3tb] Update frontend config for terminal WebSocket URL (closed 2025-12-12)
- [workspace-bc0] Create docker-compose with Traefik and internal network (closed 2025-12-12)
- [workspace-c84] Create Caddy configuration for dynamic container routing (closed 2025-12-12)
- [workspace-jxj] Create Traefik configuration for dynamic container routing (closed 2025-12-12)
- [workspace-x24] Add ttyd for web terminal to Dockerfiles (closed 2025-12-12)
- [workspace-10] Remove rounded corners across frontend UI (closed 2025-12-12)
- [workspace-11] Flatten project cards: remove shadows and set white 10% opacity (closed 2025-12-12)
- [workspace-lfu] Frontend UI Architecture - Professional & Extensible App System (closed 2025-12-09)
- [workspace-lfu.1] Design System - Professional Color Palette & Typography (closed 2025-12-09)
- [octo-k8z1.6] Frontend: Browser toolbar (URL bar, navigation buttons) (closed )
- [octo-k8z1.3] Backend: Forward input events (mouse/keyboard) to agent-browser (closed )
- [octo-k8z1.7] MCP: Add browser tools for agent control (open, snapshot, click, fill) (closed )
- [octo-k8z1.4] Frontend: Add BrowserView component with canvas rendering (closed )
