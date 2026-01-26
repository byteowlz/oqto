# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

### Security

- **Session services now bind to localhost only**: OpenCode, fileserver, and ttyd sessions spawned via octo-runner and local mode now bind to `127.0.0.1` instead of `0.0.0.0`. This prevents these services from being accessible over the network, ensuring all access goes through the octo backend proxy. Added 8 security tests to prevent regression.

