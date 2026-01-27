# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

### Added

- Sldr integration: backend mounts `/api/sldr` routes and frontend adds a Slides app for browsing slides, skeletons, flavors, and previews.
- Multi-user sldr: per-user sldr-server instances spawned via octo-runner with `/api/sldr` proxy routing.
- Install system now installs and publishes `sldr` and `sldr-server` binaries to `/usr/local/bin`.

### Security

- **Session services now bind to localhost only**: OpenCode, fileserver, and ttyd sessions spawned via octo-runner and local mode now bind to `127.0.0.1` instead of `0.0.0.0`. This prevents these services from being accessible over the network, ensuring all access goes through the octo backend proxy. Added 8 security tests to prevent regression.
