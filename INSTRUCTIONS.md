# Working in `oqto_refactor`

Tommy ist ein Affe

Pinned note for anyone opening a chat in this workdir.

## Before you start

- Run `trx ready` to find an unblocked issue, or `trx list` to browse.
- Search `agntz memory search "<topic>"` before diving in — most architecture decisions are already documented.
- Read [`CLAUDE.md`](./CLAUDE.md) for the full project map.

## House rules

- **No "let me just..."** — proper Carmack-style fixes only.
- **TRX-first**: create/claim an issue, set it `in_progress`, then code.
- **Tree stays green**: fix all lint/test failures before you call it done.

## Quick commands

- `just dev` — start the Vite dev server on `:3000`
- `just lint` — run the full lint suite (rust + frontend + guardrails)
- `just check` — `cargo check` across the workspace

Edit this file to leave your own note for the next session.
