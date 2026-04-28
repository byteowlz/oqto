# Documentation layout

This repository uses a lean docs layout with date-prefixed filenames.

## Naming convention

Active docs use:

`YYYYMMDD-topic-name.md`

Example: `20260410-canonical-protocol.md`

This makes document age visible at a glance.

## Directory structure

- `docs/active/` — current source-of-truth docs
  - `guides/`
  - `design/`
  - `debugging/`
  - `security/`
  - `examples/`
  - `skills/`
- `docs/archive/` — historical/superseded docs
  - `legacy/`
  - `history/`
  - `bugs/`
  - `reports/`
  - `fixes/`

## Rules

1. New docs go in `docs/active/...` with `YYYYMMDD-` prefix.
2. Superseded docs are moved to `docs/archive/...` (do not delete by default).
3. Update internal links when moving/renaming docs.
