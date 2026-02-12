# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

### Added

- Sldr integration: backend mounts `/api/sldr` routes and frontend adds a Slides app for browsing slides, skeletons, flavors, and previews.
- Multi-user sldr: per-user sldr-server instances spawned via octo-runner with `/api/sldr` proxy routing.
- Install system now installs and publishes `sldr` and `sldr-server` binaries to `/usr/local/bin`.
- Workspace Pi sessions: per-workspace Pi processes with idle cleanup, API endpoints, and WebSocket streaming.
- Added `/browser` and `/close-browser` slash commands to toggle the browser stream panel.
- Added `tools/test-ssh-proxy.sh` helper script to validate octo-sandbox with octo-ssh-proxy.
- Added workspace locations table to track local/remote workspace roots with active location selection.
- Added BOOTSTRAP.md onboarding instructions and workspace metadata files for main chat initialization.
- Setup now clones the octo-templates repo to a shared system path and configures onboarding templates to use it.
- Setup now updates git repos in /usr/local/share/octo/external-repos and uses the shared templates repo for project templates.
- Added feedback dropbox configuration and background sync to a private archive.
- Added onboarding bootstrap flow that creates the main workspace, writes onboarding templates, and seeds the first Pi chat session in hstry.

### Changed

- Replaced cass-backed session search with hstry search and added line-based scroll resolution for search hits.
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
- `tools/test-ssh-proxy.sh` now places its proxy socket under `~/.config/octo` so it is visible inside the sandbox.
- `tools/test-ssh-proxy.sh` now runs a host SSH test by default (use `--no-host` to skip).
- `tools/test-ssh-proxy.sh` now bypasses system SSH config during host tests to avoid permission errors.
- Workspace Pi header model switching now uses Pi RPC and shows Pi models instead of OpenCode options.
- New Pi sessions clear cached messages to avoid leaking previous session history.
- Workspace Pi API now resumes sessions when fetching state/models/WS and returns empty history when no session file exists.
- Workspace Pi UI now skips pending session IDs for Pi RPC/model/state calls to avoid 500s during optimistic creation.
- Pi RPC model switching now sends `modelId` to match pi-mono's SetModel contract.
- Chat sidebar now supports multi-select bulk deletes with immediate removal and uses the same selection bar styling as Trx.
- Chat markdown list rendering keeps markers inline and removes extra indentation inside lists.
- Onboarding templates now support configurable repo subdirectory (for octo-templates agents/ layout).
- Chat history de-duplication now collapses hstry global sessions with matching readable IDs to avoid split entries.
- Chat history now merges duplicate sessions from mixed sources using a stable key to avoid auto-rename creating extra entries.
- Pi sessions now persist canonical parts into hstry (tool calls/results preserved) with JSONL backfill for missing history.
- Pi chat view now shows a working spinner bubble immediately after sending a prompt.
- Pi chat input and empty timeline now show a working indicator while awaiting the first response.
- Pi todo extension now persists todos in the central Pi store instead of writing to repo paths.

### Security

- **Session services now bind to localhost only**: OpenCode, fileserver, and ttyd sessions spawned via octo-runner and local mode now bind to `127.0.0.1` instead of `0.0.0.0`. This prevents these services from being accessible over the network, ensuring all access goes through the octo backend proxy. Added 8 security tests to prevent regression.
- Sandbox deny-read masking now handles file paths (for example, `systemctl`) without bubblewrap tmpfs errors, and deny rules are enforced even when the workspace is the home directory.
