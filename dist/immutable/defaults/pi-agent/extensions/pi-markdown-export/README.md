# pi-markdown-export

Export Pi sessions to Markdown — the current session, every session for a
directory, or an interactive pick — with config-driven redaction applied before
anything is written.

## Commands

| Command | What it does |
| --- | --- |
| `/export-md [filename.md]` | Export the **current** session. No arg → `pi-session-<timestamp>.md` in the cwd; relative arg → under the cwd; absolute arg → that path. |
| `/export-md-all [--subdirs]` | Export **all** sessions whose working directory is the cwd (and, with `--subdirs`, any subdirectory) to the export dir, one `.md` per session. |
| `/export-md-pick [--subdirs]` | Open a TUI **multi-select picker** (fuzzy filter, space to toggle) and export the chosen sessions to the export dir. |

Bulk and picker exports go to `exportDir` (default `./pi-session-exports`).

### Picker keys

`↑/↓` navigate · type to fuzzy-filter (id / project / date / title) · `Space`
toggle · `Tab` toggle + next · `Ctrl+A` toggle all (filtered) · `Enter` export
(checked set, or the highlighted row when nothing is checked) · `Esc` cancel.

## Output format

- Session title as H1 (the session name, else the first user message)
- Export timestamp
- Message blocks grouped by role (`User`, `Assistant`, `Tool`, `System`)
- Assistant thinking included only when `includeThinking` is true

### Readability / verbosity

By default exports are an **outline**: user + assistant text, with each assistant
tool call shown as a compact one-liner (a `` `- `read(path)` `` list item), and
tool result bodies (file dumps, command output — usually the overwhelming bulk)
omitted. In practice this cuts a typical session by ~95%. The renderer never
introduces emoji or other decoration into the export — only what was in the
session text is kept.

Three config knobs tune this:

| Key | Default | Effect |
| --- | --- | --- |
| `includeToolCalls` | `true` | Show compact `- \`tool(arg)\`` lines. Set `false` for pure conversation. |
| `includeToolResults` | `false` | Include tool output bodies. Turning this on restores the noise — pair it with `maxCharsPerMessage`. |
| `maxCharsPerMessage` | `0` | Cap each message at N chars (`0` = unlimited); truncated messages get a `… (truncated, N chars)` marker. |

Common profiles:

- **Outline** (default): `includeToolResults: false`, `includeToolCalls: true`.
- **Conversation only**: also `includeToolCalls: false`.
- **Truncated transcript**: `includeToolResults: true`, `maxCharsPerMessage: 500`.
- **Full transcript**: `includeToolResults: true`, `maxCharsPerMessage: 0`.

## Configuration

Optional `markdown-export.json`, searched in order (first match wins):

1. `./markdown-export.json` (cwd)
2. `./.pi/markdown-export.json` (project-local)
3. `~/.pi/agent/markdown-export.json` (global)

See `markdown-export.schema.json` and `markdown-export.example.json`.

| Key | Default | Description |
| --- | --- | --- |
| `exportDir` | `./pi-session-exports` | Where bulk/picker exports are written (relative to cwd). |
| `includeSubdirs` | `false` | Include subdirectories of the cwd in bulk export / picker (the `--subdirs` flag forces this on per-invocation). |
| `includeThinking` | `false` | Include assistant thinking blocks. |
| `includeToolCalls` | `true` | Show compact `- \`tool(arg)\`` lines (see Readability). |
| `includeToolResults` | `false` | Include tool output bodies (see Readability). |
| `maxCharsPerMessage` | `0` | Per-message character cap, `0` = unlimited (see Readability). |
| `sessionsDir` | `null` | Override pi's sessions base dir. Falls back to `$PI_SESSIONS_DIR` then `~/.pi/agent/sessions`. Leading `~` expanded. |
| `replacements` | `[]` | Find/replace rules (see below). |
| `redactionCommands` | `[]` | External redaction CLIs (see below). |

### Redaction pipeline

Every export — including `/export-md` — is run through the pipeline before being
written: **replacements first, then each redaction command in order**, each
seeing the previous step's output.

#### Replacements

Literal by default; set `"regex": true` to treat `find` as a regular expression
(with optional `flags`, default `g`, and `$1`-style group refs in `replace`).

```json
{ "replacements": [
  { "name": "company", "find": "Acme Corporation", "replace": "[COMPANY]" },
  { "find": "\\bsk-[A-Za-z0-9]{16,}\\b", "replace": "[KEY]", "regex": true, "flags": "g" }
] }
```

#### Redaction commands

Each command runs over the current export text. Two `args` placeholders are
substituted: `{file}` (a temp file holding the text) and `{report}` (a temp path
the tool may write a JSON report to).

- **`mode: "scan"`** (default) — for detectors like **gitleaks** and
  **trufflehog**. Secret strings are parsed from the tool's JSON output (stdout
  or `{report}` — keys `Secret`, `Raw`, `RawV2`, `Match`) and **masked** in the
  text (`maskWith`, default `[REDACTED]`). If the tool flags secrets but emits
  nothing maskable, `onFinding` decides: `warn` (default, keep + warn), `skip`
  (don't write this export), or `mask` (keep + warn to review).
- **`mode: "filter"`** — the command *transforms* the text. The new content is
  taken from the command's **stdout**, or from `{file}` when `"inPlace": true`.
  Set `"stdin": true` to pipe the text to the command's stdin instead.

A missing executable or non-zero exit is logged and **skipped** unless
`"required": true`, in which case it aborts the export. `timeoutMs` defaults to
30000.

```json
{ "redactionCommands": [
  {
    "name": "gitleaks",
    "command": "gitleaks",
    "args": ["detect", "--no-banner", "--no-git", "--report-format", "json",
             "--report-path", "{report}", "--source", "{file}"],
    "mode": "scan",
    "maskWith": "[REDACTED-SECRET]"
  },
  {
    "name": "trufflehog",
    "command": "trufflehog",
    "args": ["filesystem", "{file}", "--json", "--no-update"],
    "mode": "scan"
  }
] }
```

## Notes

- Session-to-directory matching reads each session's recorded `cwd` from its
  first JSONL record, so it is robust against pi's lossy directory-name encoding.
- Export filenames are `<timestamp>__<shortid>.md`, prefixed with an encoded
  project segment when `--subdirs` is active (to avoid cross-project collisions).
