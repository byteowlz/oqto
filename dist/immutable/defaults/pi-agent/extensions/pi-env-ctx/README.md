# pi-env-ctx

Export Pi-native `AGENT_CTX_*` metadata into `process.env` so spawned tools/commands can identify the active harness, session, and model.

## What this extension sets

This extension owns only the following fields:

- `AGENT_CTX_VERSION=1`
- `AGENT_CTX_HARNESS=pi`
- `AGENT_CTX_HARNESS_SESSION_ID` (current Pi session id)
- `AGENT_CTX_MODEL` (`provider/id`, e.g. `anthropic/claude-3-7-sonnet`)
- `AGENT_CTX_SESSION_NAME` (optional display name, may appear/change later)

Unknown values are left **unset** (not empty strings).

## Mutability and lifecycle

- On `session_start`: initializes all owned vars.
- On `before_agent_start` and `turn_start`: refreshes mutable fields (`AGENT_CTX_HARNESS_SESSION_ID`, `AGENT_CTX_SESSION_NAME`, `AGENT_CTX_MODEL`) before active turn/tool work.
- On `model_select`: updates `AGENT_CTX_MODEL`.
- On `session_tree` and `turn_end`: refreshes `AGENT_CTX_HARNESS_SESSION_ID` and `AGENT_CTX_SESSION_NAME`.
  - This ensures session id/name stay correct when switching to a different session in the same Pi process.

## Ownership boundary

Not owned by this extension (runner/sandbox responsibility):

- `AGENT_CTX_WORKSPACE`
- `AGENT_CTX_PLATFORM_SESSION_ID`
- `AGENT_CTX_USER_ID`
- any other platform/runtime-specific identity vars

## Security caveat

`AGENT_CTX_*` values are metadata for context/search/routing. They are **not** a security boundary and must not be trusted for authorization.

## Quick verification

After enabling the extension, run:

```bash
node -e 'console.log({
  AGENT_CTX_VERSION: process.env.AGENT_CTX_VERSION,
  AGENT_CTX_HARNESS: process.env.AGENT_CTX_HARNESS,
  AGENT_CTX_HARNESS_SESSION_ID: process.env.AGENT_CTX_HARNESS_SESSION_ID,
  AGENT_CTX_MODEL: process.env.AGENT_CTX_MODEL,
  AGENT_CTX_SESSION_NAME: process.env.AGENT_CTX_SESSION_NAME,
})'
```
