# pi-history-search

Lets a pi agent **search its own session history** efficiently, via a SQLite
FTS5 index. Two LLM-callable tools — `HistorySearch` and `HistoryRead` — so the
agent can recall earlier decisions, prior solutions, file paths, error messages,
or what was already tried, instead of re-deriving them.

Designed to integrate cleanly with [oqto](https://github.com/byteowlz/oqto) and
to work just as well with plain pi. No external LLM / OpenRouter dependency.

## Tools

### `HistorySearch`

Full-text, BM25-ranked search over past sessions.

| Param | Type | Notes |
|---|---|---|
| `query` | string (optional) | Search terms. **Omit/empty → most recent sessions** (for "what did we do here/recently"). |
| `scope` | `"project"` \| `"all"` | Default `"project"`. `"all"` also searches other projects the environment exposes. |
| `project` | string | With `scope:"all"`, keep only sessions whose project label contains this. |
| `roleFilter` | `all` \| `conversation` \| `user` \| `assistant` \| `tool` | **Default `conversation`** (user+assistant) — best signal, skips the tool-output noise that otherwise dominates. Use `all` to also search tool output (error messages, file paths, command output), or `tool` for only that. |
| `limit` | number | Max sessions (default from config). |

Returns matching sessions, each with a `sessionId`, a project + timestamp, and
snippets tagged by `role` and `msgIndex`.

### `HistoryRead`

Pull fuller context from one session returned by `HistorySearch`.

| Param | Type | Notes |
|---|---|---|
| `sessionId` | string (required) | From a `HistorySearch` hit. |
| `around` | number | Window of messages centered on this `msgIndex`. |
| `before` / `after` | number | Window size (default 3 each). |
| `query` | string | Return every message in the session matching these terms. |
| `view` | `outline` \| `transcript` | Whole-session rendering (ignored with `around`/`query`). **Default `outline`** = user+assistant only, tool noise dropped (compact recall, ~60% fewer chars). `transcript` = every non-empty message. |
| `roleFilter` | `all` \| `conversation` \| `user` \| `assistant` \| `tool` | Restrict returned roles (query mode, or override the outline/transcript default). |
| `maxChars` | number | Per-message cap (whole-session: total budget split across messages). Default 2000. |

With neither `around` nor `query`, returns the whole session — a compact
`outline` by default, or the full `transcript`.

### TUI overlay (humans)

You can search history interactively, not just via the agent tools:

- **`Ctrl+Shift+F`** — open the live search overlay.
- **`/history`** — open the same overlay (in an interactive TUI). `/history <query>`
  opens it seeded with that query.

In the overlay:

| Key | Action |
|---|---|
| type | live-filter the current project's history |
| `↑` / `↓` | move selection (empty query shows recent sessions) |
| `Enter` | preview the selected session (jumps to the first match) |
| `↑` / `↓` / `PgUp` / `PgDn` | scroll the preview |
| `Esc` / `←` | back to results (from preview) |
| `Ctrl+U` | clear the query |
| `Esc` | close |

Headless (rpc/print) `/history <query>` prints ranked results instead; `/history
stats` and `/history reindex` manage the index in any mode.

## Where the index lives — and why

The index is **colocated per project**, inside pi's own session directory:

```
~/.pi/agent/sessions/
  --home-user-code-myapp--/
      2026-...session.jsonl
      .history/  →  .pi-history/index.db   (+ -wal, -shm)
```

The guiding invariant: **the index is reachable exactly when the sessions it
indexes are reachable.** That choice falls out of the "works with and without
oqto" requirement:

- **Without oqto** — it depends only on pi's native session layout, so it just
  works; one index per project.
- **With oqto's sandbox** — when the sandbox restricts an agent to its own
  sessions, the matching index is restricted with it. There is no shared global
  database that a sandboxed agent could open to read other projects' history.

The **current project** is indexed read-write and incrementally (only files
whose mtime changed). Other projects (reached only via `scope:"all"`) are queried
read-only when an index already exists, or scanned live otherwise — the
extension never writes into another project's directory. Each project's index is
maintained by agents working in that project.

## Configuration

Loaded from the first match of: `./history-search.json`,
`./.pi/history-search.json`, `~/.pi/agent/history-search.json`. See
`history-search.schema.json` and `history-search.example.json`.

| Key | Default | Description |
|---|---|---|
| `enabled` | `true` | Master switch. |
| `sessionsDir` | `null` | Override pi's sessions base (else `$PI_SESSIONS_DIR`, else `~/.pi/agent/sessions`). |
| `indexOnStart` | `true` | Incrementally index the current project on `session_start`. |
| `includeToolResults` | `true` | Index tool-result messages too. |
| `maxResults` | `10` | Default sessions per search. |
| `snippetsPerSession` | `3` | Snippets per session. |

## How it works

1. On `session_start`, the current project's index is brought up to date
   incrementally (by mtime), off the startup critical path.
2. `extractMessages` parses each session JSONL: user text, assistant text (no
   thinking / tool calls), and optionally tool results — one stable ordinal per
   message so search hits and `HistoryRead` line up.
3. Text is chunked (~4 KB) into an FTS5 table (`porter unicode61`). Searches use
   `MATCH` with BM25 ranking, deduplicated per session, with highlighted
   snippets.

## Dependencies

None beyond pi itself. The index uses Node's built-in `node:sqlite` (FTS5 is
compiled in) — no native module to build or version-match. Requires Node ≥ 22.5
(where `node:sqlite` is available). On older Node, the SQLite index is skipped
and search transparently falls back to a live JSONL scan.
