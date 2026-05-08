# oqto-history

## Responsibility

History and oqto-log storage boundaries shared by the server and runner.

## Non-goals

No HTTP handlers, runner process supervision, user provisioning, or frontend transport logic.

## Depends on

Storage, serialization, and small utility crates needed for durable history operations.

## Used by

`oqto` today via compatibility re-exports. `oqto-runner` will depend on this crate once runner daemon history calls move out of the server crate.

## Migration notes

Extraction is incremental. `oqto_log::ids` and `oqto_log::paths` moved first because they are pure helpers. Store/projector/importer APIs still live in `oqto` until their remaining dependencies are untangled.
