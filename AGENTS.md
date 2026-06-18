# Oqto agent guide

Oqto is a self-hosted workspace for AI coding agents. Keep this file short; put runbooks/design detail in docs, skills, or `repo-analysis/`.

## Agent skills

### Issue tracker

Issues are tracked with `trx` in this repo; use `trx ready/list/show/create/update/close` and `trx dep block/unblock`, not GitHub Issues. See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context layout: project domain language lives in `CONTEXT.md`; architectural decisions live in `docs/adr/`. See `docs/agents/domain.md`.

## Rules

- Before changing code/config/docs: `trx ready` or `trx list` -> reuse/create issue -> `trx update <id> --status in_progress`; close/update it when done. Ambiguous git/trx commands are read-only first.
- Search `agntz memory` before unfamiliar work. Add memories only for reusable architecture/interface/debugging lessons.
- Use `CONTEXT.md` for project domain language. `docs/adr/` is the canonical decision log; read past ADRs before architecture changes and add/update ADRs for new decisions.
- Repeated failures must become mechanisms, not reminders: propose or add a lint, test, doctor, checklist, or skill when a mistake recurs.
- No hacky fixes or legacy shims. Understand the root cause, respect the architecture, and delete dead compatibility paths when safe.
- Architecture seams: actions go through runners; runner sessions use `oqto-log` as durable history authority; hstry is legacy/interop only; memory goes through mmry; do not bypass stores with ad-hoc DB/file writes.
- Session identity is sacred: keep `platform_id` and `external_id` distinct; never persist `pending-*`/`tmp:*`; Pi owns JSONL session files and Oqto must not write them.
- Chat/session changes require proof: name the durable authority, event source, ID mapping, reconnect/reload behavior, and regression test/trace. Never reconcile messages by text, index, array length, or visible order.
- High-risk domains need their checklist before editing: chat persistence, sessions/forks/import, sandbox/security, setup/deploy, EAVS/user config, protocol/generated types, frontend event state.
- User-owned config is preserve-first: anything under `~/.pi`, `~/.config`, per-user homes, model lists, or generated user settings needs backup + diff + merge semantics; never overwrite unknown entries.
- Config/docs must match runtime truth. New config modes require implementation, tests, and fail-closed behavior for unsupported values.
- Use `just`/package scripts for gates. Touched production code must be formatted, linted, tested, and warning-free; no “pre-existing” warning handoff without an explicit accepted-debt rationale.
- Frontend: avoid raw `useEffect`; use approved hooks or an inline guardrail exception. Keep generated TypeScript types fresh after protocol/Rust type changes.
- Rust: prefer `anyhow::Result` + context; no `unwrap`/`expect` in production paths unless explicitly justified and allowed by guardrails.
- Deploy/release: use `just bump` for versions; package/deploy must pass manifest/artifact checks; release tags must match declared Cargo/package versions.
- Debug UI with `DISPLAY=:0 agent-browser ...`; frontend dev is `localhost:3000`; use tmux logs for backend/frontend/runner.
- If Claude Code cargo fails with EPERM/statx/timer_create on git deps, treat it as harness sandbox capability, not code failure; use standalone rustfmt/ast-grep locally and run cargo gates in an unsandboxed lane.
- Keep documentation current when architecture changes, but prefer links over duplicating specs here.
- Before declaring done, audit the objective against real evidence: files changed, commands run, tests/gates, known gaps, and trx status. Always update trx status for items that have been touched in a session.
