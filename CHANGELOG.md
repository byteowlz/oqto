# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

### Added

- Chat "Jump to bottom" button appears when user has scrolled up; clicking it pins back to the bottom and resumes auto-scroll.

- Sldr integration: backend mounts `/api/sldr` routes and frontend adds a Slides app for browsing slides, skeletons, flavors, and previews.
- Multi-user sldr: per-user sldr-server instances spawned via oqto-runner with `/api/sldr` proxy routing.
- Install system now installs and publishes `sldr` and `sldr-server` binaries to `/usr/local/bin`.
- Workspace Pi sessions: per-workspace Pi processes with idle cleanup, API endpoints, and WebSocket streaming.
- Added `/browser` and `/close-browser` slash commands to toggle the browser stream panel.
- Added `tools/test-ssh-proxy.sh` helper script to validate oqto-sandbox with oqto-ssh-proxy.
- Added workspace locations table to track local/remote workspace roots with active location selection.
- Added BOOTSTRAP.md onboarding instructions and workspace metadata files for main chat initialization.
- Setup now clones the oqto-templates repo to a shared system path and configures onboarding templates to use it.
- Setup now updates git repos in /usr/local/share/oqto/external-repos and uses the shared templates repo for project templates.
- Added feedback dropbox configuration and background sync to a private archive.
- Added onboarding bootstrap flow that creates the main workspace, writes onboarding templates, and seeds the first Pi chat session in hstry.
- Added JSONL audit logging for authenticated HTTP requests and WebSocket commands, configurable via logging.audit_* settings.

### Removed

- Removed OpenCode chat history runner protocol endpoints (ListOpencodeSessions, GetOpencodeSession, GetOpencodeSessionMessages, UpdateOpencodeSession) and their request/response types.
- Removed OpenCode chat history runner client methods and handler implementations.
- Removed OpenCode disk-based session fallbacks from chat API handlers (get_chat_session, update_chat_session, get_chat_messages) -- all chat history now flows through hstry.
- Renamed protocol types: OpencodeMessage -> ChatMessageProto, OpencodeMessagePart -> ChatMessagePartProto.
- Added UpdateWorkspaceChatSession runner endpoint to replace UpdateOpencodeSession for session title updates via hstry SQLite.

### Changed

- Chat auto-scroll now pins to the bottom on every streaming token update, not just on new message arrival, eliminating the jumping/flickering during streaming. Uses direct `scrollTop` assignment instead of `scrollIntoView` to prevent scroll jank.
- Chat scroll position is preserved via a ref mirror to avoid stale closure bugs that could cause missed auto-scrolls.

- Replaced cass-backed session search with hstry search and added line-based scroll resolution for search hits.
- Documented archlinux CORS origins in the example Oqto config.
- Renamed CASS search types and comments to hstry in the API and frontend.
- Pi session resolution now de-duplicates JSONL files by session id, prefers the newest file, and persists session metadata into hstry to prevent split chats.
- Sidebar multi-select now anchors the active session, and minimal tool-call badge sizing matches the compact badge style.
- Sidebar deletes now confirm before removing chats, and file tree refreshes are throttled to reduce backend spam.
- Chat history cache now updates on every change, and the agent-working indicator is only shown in the message bubble.
- Session UI now renders non-OpenCode sessions with the Pi chat view, and new chats default to Pi workspace sessions.
- Chat history now includes workspace Pi sessions, and the status bar shows the active Pi model for main/workspace chats.
- Pi model switching is now gated to idle sessions; the model picker and `/model` command are disabled while Pi is streaming or compacting.
- Pi settings UI now writes workspace `.pi` overrides (settings/models) while global settings remain in `~/.pi/agent`.
- Pi settings panel now matches the OpenCode settings layout with a single, flat view (no tabs).
- Pi settings model search keeps focus while typing in the selector input.
- Settings editor now allows clearing numeric fields without browser "null" input errors.
- `tools/test-ssh-proxy.sh` now auto-adds `~/.ssh/id_ed25519` if the agent has no keys loaded.
- `tools/test-ssh-proxy.sh` now starts an `ssh-agent` if `SSH_AUTH_SOCK` is unset.
- Runner now creates hstry conversation metadata as soon as Pi reports the native session id.
- Runner now migrates hstry history on session re-key so existing chats keep their full history.
- Runner now routes steer/follow_up to prompt when Pi sessions are idle, starting, stopping, or aborting, ensuring new turns are not dropped after reconnects or after Pi exits.
- New Pi chats now use an oqto- prefixed provisional ID so Oqto IDs never look like Pi UUIDs.
- Multi-user chat history now lists Pi sessions from hstry via the runner and fetches Pi messages from hstry for workspace chats.
- Updated hstry install template to point Pi adapter sources at ~/.pi/agent/sessions.
- `tools/test-ssh-proxy.sh` now places its proxy socket under `~/.config/oqto` so it is visible inside the sandbox.
- `tools/test-ssh-proxy.sh` now runs a host SSH test by default (use `--no-host` to skip).
- `tools/test-ssh-proxy.sh` now bypasses system SSH config during host tests to avoid permission errors.
- Workspace Pi header model switching now uses Pi RPC and shows Pi models instead of OpenCode options.
- Model selection now persists while sessions are still starting and applies once sessions are created to prevent new/reconnected chats from defaulting to the fallback model.
- New Pi sessions clear cached messages to avoid leaking previous session history.
- Workspace Pi API now resumes sessions when fetching state/models/WS and returns empty history when no session file exists.
- Workspace Pi UI now skips pending session IDs for Pi RPC/model/state calls to avoid 500s during optimistic creation.
- Pi RPC model switching now sends `modelId` to match pi-mono's SetModel contract.
- Chat sidebar now supports multi-select bulk deletes with immediate removal and uses the same selection bar styling as Trx.
- Chat markdown list rendering keeps markers inline and removes extra indentation inside lists.
- Onboarding templates now support configurable repo subdirectory (for oqto-templates agents/ layout).
- Chat history de-duplication now collapses hstry global sessions with matching readable IDs to avoid split entries.
- Chat history now merges duplicate sessions from mixed sources using a stable key to avoid auto-rename creating extra entries.
- Pi sessions now persist canonical parts into hstry (tool calls/results preserved) with JSONL backfill for missing history.
- Pi chat view now shows a working spinner bubble immediately after sending a prompt.
- Pi chat input and empty timeline now show a working indicator while awaiting the first response.
- Pi todo extension now persists todos in the central Pi store instead of writing to repo paths.
- Trx view now disables the hide-closed filter when viewing closed issues so completed items can be listed.
- Pi list_sessions now returns the resolved session ID (Pi native ID when known) to avoid reattach mismatches.
- Linux user ownership now accepts sanitized GECOS fields without a colon to avoid login failures after chfn.
- Setup now installs typst and slidev globally and ensures fd is available when only fdfind is installed.
- Added an admin Unix socket for oqtoctl with peer-credential checks and automatic CLI fallback for local root access.

### Security

- **Session services now bind to localhost only**: OpenCode, fileserver, and ttyd sessions spawned via oqto-runner and local mode now bind to `127.0.0.1` instead of `0.0.0.0`. This prevents these services from being accessible over the network, ensuring all access goes through the oqto backend proxy. Added 8 security tests to prevent regression.
- Chat history listing now uses per-user runner data in multi-user mode, skips direct hstry fallback for message retrieval, and the backend no longer starts a shared hstry client when linux user isolation is enabled.
- Per-user oqto-runner systemd services now bind to the expected /run/oqto/runner-sockets/<user>/oqto-runner.sock path and auto-update when the unit file changes.
- Per-user mmry configs are now initialized/updated to use the central mmry embedding endpoint and per-user runners auto-enable hstry/mmry services.
- Added admin API and oqtoctl command to batch sync per-user configs.
- Login now ensures the per-user oqto-runner is started (including lingering) so new users do not require a Linux login to access isolated history.
- Sandbox deny-read masking now handles file paths (for example, `systemctl`) without bubblewrap tmpfs errors, and deny rules are enforced even when the workspace is the home directory.
