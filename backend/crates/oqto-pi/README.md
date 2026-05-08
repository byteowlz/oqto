# oqto-pi

## Responsibility

Pi wire protocol types and Pi session-file helper utilities shared by the server and runner code.

## Non-goals

No process management, runner socket transport, HTTP handlers, persistence orchestration, or Pi subprocess lifecycle management.

## Depends on

Serialization, regex, logging, and async utility crates only.

## Used by

`oqto` today, and `oqto-runner` once runner daemon ownership moves out of the server crate.

## Migration notes

This crate is a prerequisite for removing the `oqto-runner -> oqto` dependency without creating a Cargo dependency cycle.
