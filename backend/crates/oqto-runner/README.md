# oqto-runner

## Responsibility

Per-user agent daemon. It owns agent harness processes, translates harness-native events into canonical events, and exposes the runner socket protocol used by the backend.

## Non-goals

No frontend HTTP API, no backend admin API, and no direct database authority outside explicitly designed history/store abstractions.

## Depends on

Canonical protocol types, host/process helpers as needed, and harness integration code.

## Used by

The `oqto` backend via runner socket client paths and by deployed per-user systemd services.

## Migration notes

The runner daemon core has already been separated from `oqto`. Backend-side runner client/protocol facade code should become `oqto-runner-client` in a future extraction.
