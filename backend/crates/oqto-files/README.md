# oqto-files

## Responsibility

Workspace file access service and binary. It serves file operations used by Oqto sessions and the frontend file channel.

## Non-goals

No session orchestration, no user provisioning, and no general backend API routing.

## Depends on

File-system, HTTP/WebSocket, and serialization crates required for file serving.

## Used by

`oqto` deploy/runtime flows and local/runner session infrastructure.

## Migration notes

Keep file access concerns here. If a feature needs chat/session semantics, implement those in session/API layers and call this service through a narrow interface.
