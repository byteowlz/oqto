# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

### Added

- Sldr integration: backend mounts `/api/sldr` routes and frontend adds a Slides app for browsing slides, skeletons, flavors, and previews.
- Multi-user sldr: per-user sldr-server instances spawned via octo-runner with `/api/sldr` proxy routing.
- Install system now installs and publishes `sldr` and `sldr-server` binaries to `/usr/local/bin`.
- Workspace Pi sessions: per-workspace Pi processes with idle cleanup, API endpoints, and WebSocket streaming.
- Added `tools/test-ssh-proxy.sh` helper script to validate octo-sandbox with octo-ssh-proxy.

### Changed

- Replaced cass-backed session search with hstry search and added line-based scroll resolution for search hits.
- Renamed CASS search types and comments to hstry in the API and frontend.
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

### Security

- **Session services now bind to localhost only**: OpenCode, fileserver, and ttyd sessions spawned via octo-runner and local mode now bind to `127.0.0.1` instead of `0.0.0.0`. This prevents these services from being accessible over the network, ensuring all access goes through the octo backend proxy. Added 8 security tests to prevent regression.
- Sandbox deny-read masking now handles file paths (for example, `systemctl`) without bubblewrap tmpfs errors, and deny rules are enforced even when the workspace is the home directory.
