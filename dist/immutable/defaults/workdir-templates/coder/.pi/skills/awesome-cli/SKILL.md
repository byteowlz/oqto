---
name: awesome-cli
description: "Design and implement delightful, scriptable command-line tools: subcommands, composable stdin/stdout/stderr, stable --json/--yaml, great help/errors, safe destructive ops, config precedence (flags>env>config), TTY-aware color/progress, completions, and tests."
short-description: Build high-quality CLIs.
version: "1.0"
compatibility: Any OS and language. Assumes you can run the CLI in a terminal to verify TTY behavior, exit codes, and piping.
---

# Awesome CLI

## When to use this skill

Use when designing or implementing any CLI (new tool or new subcommand), especially if you need:

- subcommand/flag structure
- help text + examples
- machine output (`--json`, `--jsonl`)
- config/env precedence and XDG paths
- destructive-operation safety (`--dry-run`, confirmations)
- TTY-aware color/progress/paging
- completions + schemas + testing strategy

## Agent workflow (do this order)

1. **Interface sketch first (no code yet)**
   - Command tree (top-level + subcommands) with one-line intent for each.
   - Inputs per command: args vs flags vs stdin vs files.
   - Outputs per command: stdout vs stderr, human vs `--json/--jsonl`, and exit codes.
   - Safety plan for any state change / deletion.
   - Config sources + precedence + merge rules.
2. **Choose an argument parser library**
   - Must support subcommands, auto-help, validation, and "did you mean …" suggestions.
   - Examples by ecosystem (pick what matches the project): argparse/click/typer (Python), clap (Rust), Cobra (Go), picocli (Java), oclif (Node).
3. **Implement the skeleton**
   - Global flags, consistent help, and a stub for each subcommand.
4. **Implement each subcommand**
   - Follow the non-negotiables (streams, exit codes, teachy errors, TTY rules, machine mode contract).
5. **Add formats, schemas, tests, and polish**
   - Stable schemas for `--json/--jsonl`, schema command, golden tests, completions, docs/man snippet, versioning.

---

# Non-negotiables

## Exit codes

- `0` success.
- Non-zero on failure. Prefer a documented set:
  - `2` usage / invalid args (parser errors, missing required input)
  - `1` runtime error (unexpected but handled)
  - Optional (only if useful): `3` network/timeout, `4` partial failure, `130` interrupted (SIGINT)
- Never dump stack traces by default. Only show them with `--trace` (or in `--debug` if explicitly requested).

## stdout vs stderr

- **stdout**: primary results and anything machine-readable (`--json`, `--jsonl`, `--plain`).
- **stderr**: errors, warnings, logs, progress indicators, debug/trace output.

## Machine mode contract (critical)

If a machine format flag is set (`--json`, `--jsonl`, or `--format json|jsonl`):

- **stdout MUST be parseable as that format, even on failure**.
- stdout must contain **only** the machine output (no banners, no "done", no progress, no warnings).
- stderr:
  - default: only brief human error text (or silent—pick one and document it for the tool)
  - `--debug/--trace`: allowed to print diagnostics, but never to stdout

## Composability

- Prefer subcommands for verbs (`tool add`, `tool list`).
- Read from stdin when input is omitted or explicitly requested (e.g., `--input -`).
- Write results to stdout; keep stderr "clean enough" to not break pipes.
- If a command expects piped input but stdin is a TTY: **print concise help and exit 2** (don't hang).

## Files: support `-`

If an input/output is a file:

- Accept `-` to mean stdin/stdout.

## Broken pipe policy (SIGPIPE / EPIPE)

- If stdout is closed by the consumer (e.g., `tool list | head`), handle EPIPE/SIGPIPE gracefully:
  - do not print scary errors by default
  - exit 0 (or a documented benign code) and keep cleanup bounded

---

# Command structure & naming

## Subcommands

- Use either `noun verb` or `verb noun`; pick one and stay consistent.
- Keep each subcommand focused and composable.
- Avoid ambiguous pairs (`update` vs `upgrade`).
- Don't allow implicit abbreviations. If you add aliases, make them explicit and stable.

## Args vs flags

- Prefer flags for clarity and future-proofing.
- Reserve positional args for the obvious primary action only.

---

# Standard flag kit (apply across subcommands)

- `-h, --help`
- `-V, --version` (print semver; note deprecations)
- `-q, --quiet`
- `-v` (stackable; increases verbosity)
- `--debug` (human-readable diagnostics)
- `--trace` (max detail; may include stack traces)
- `--format table|plain|json|jsonl` (recommended; see Output & formats)
  - If you keep convenience flags: `--json` == `--format json`, `--jsonl` == `--format jsonl`
- `--no-color` (honor `NO_COLOR`; allow `FORCE_COLOR`)
- `--dry-run`
- `-y, --yes` (scriptable confirmation)
- `--force` (override safety checks; see Safety section)
- `--no-input` (never prompt; fail with guidance)
- `--config PATH`
- `--no-progress`
- `--timeout SEC`
- `--parallel N`
- Optional batch behavior: `--fail-fast` and/or `--keep-going`

---

# Output & formats

## Default output (human)

- Brief by default, but never "silent and scary" for state-changing commands.
- If you change state: say what changed (and where).
- Avoid output that depends on terminal width for correctness.
- If tables are used, provide `--plain` / machine formats to avoid scraping.

## `--plain`

- Minimal formatting, no wrapping, no color.
- Prefer 1 record per line if the output is list-like.
- When possible, ensure deterministic ordering.

## Machine output: `--json`

Treat as an API:

- Stable schema; additive changes only.
- Prefer a predictable envelope:
  - List: `{"ok":true,"items":[...], "meta":{...}}`
  - Single: `{"ok":true,"result":{...}, "meta":{...}}`
  - Failure: `{"ok":false,"error":{"code":"...","message":"...","details":{...}}, "meta":{...}}`
- Deterministic ordering where possible (especially when `--parallel` is used).
- Document schema and provide at least one example in `help <subcmd>`.

## Machine output: `--jsonl` / `--ndjson`

For large lists and streaming:

- Each line is a standalone JSON object.
- Recommended conventions:
  - item line: `{"type":"item", ...}`
  - optional final meta line: `{"type":"meta", ...}`
  - failure: either emit a single `{"type":"error", ...}` line and exit non-zero, or emit nothing and exit non-zero (pick and document)

## Schema publishing

- Provide schemas for machine output.
- Recommended command:
  - `tool config schema` (and optionally `tool <subcmd> schema`)
- Schema should be JSON Schema (version documented), stable, and versioned with the CLI.

---

# Help & discoverability

## Help behavior

- `tool`, `tool -h`, `tool --help` should be fast and show concise help if args are missing.
- `tool <subcmd> --help` shows rich help with examples.
- Prefer `tool help` and `tool help <subcmd>` if a git-style UX fits.

## Help content (lead with examples)

Include, in this order:

1. One-line summary
2. Common examples
3. Options (common first)
4. Inputs/outputs (stdin/stdout, formats, machine mode contract)
5. Notes on config/env and exit codes
6. Support path (issues/docs)

## Completions

Provide: `tool completions bash|zsh|fish|powershell`

- Generate from the same parser metadata as help so they never drift.

## Docs snippet / man page

- Provide either a `tool docs` (prints a README/man snippet) or ship a generated man page.

---

# Errors that teach

On failure, output to **stderr** (human mode):

1. What happened (1 line)
2. Cause (if known)
3. Fix (concrete next step)
4. Hints (near-miss flags/values/subcommands)

Template:

- `error: <what>`
- `code: <STABLE_ERROR_CODE>`
- `cause: <why>`
- `fix: <command/flag/path>`
- `hint: did you mean ...?`

Avoid log-level prefixes (`ERR`, `WARN`) unless `--debug/--trace`.

In `--json` mode:

- Populate `error.code` with the same stable error code.

---

# Destructive operations (safety)

## Required behaviors

- Always support `--dry-run` for anything that deletes/modifies remote or bulk data.
- Prompt only on TTY and only if `--no-input` is not set.
- Provide `--yes` for scripts (no prompting).
- Provide `--force` to override safety checks (e.g., "outside allowed dir", "resource exists", "non-empty delete").

## Semantics (make these explicit and consistent)

- `--yes`: answers yes to interactive confirmations.
- `--no-input`: never prompt; if confirmation would be required, exit 2 with guidance.
- `--force`: bypasses safety checks; does **not** imply `--yes` unless you explicitly document that it does.

## Escalate confirmation by risk

- Mild: maybe no prompt.
- Moderate: `Proceed? [y/N]`
- Severe: require typing the resource name, or `--confirm <name>`.

## Ctrl-C behavior

- Exit quickly on SIGINT; keep cleanup bounded.

---

# Config & environment

## Precedence (must be consistent and documented)

**flags > environment > config file > defaults**

## Locations

- Unix: XDG (`$XDG_CONFIG_HOME` / `~/.config/<tool>/config.json`)
- Windows: `%APPDATA%\<Tool>\config.json` (or platform conventions)
- Always allow override with `--config PATH`.
- Optional: `--config -` to read config from stdin (document clearly if supported).

## Merge and layering rules

Define clearly:

- Does the tool support multiple config files (system/user/project)? If yes, define load order.
- How are values merged?
  - Scalars: later overrides earlier.
  - Objects/maps: deep-merge by key (recommended).
  - Lists: replace by default (recommended), unless you explicitly support append semantics.

## Commands

- `tool config init`:
  - Writes a commented template
  - Never overwrites without `--force`
- `tool config show`:
  - Prints effective config
  - Includes the source of each value (flag/env/file/default)
- `tool config paths`:
  - Prints resolved config path(s), data dir, cache dir, log dir (if applicable)
- `tool config schema`:
  - Prints JSON Schema for the config file
  - Document schema versioning and compatibility guarantees

## Environment variables

Standardize:

- Prefix all env vars: `<TOOLNAME>_...`
- Use consistent mapping from flags to env vars (document types and parsing rules).
- Never print secrets by default; redact in debug/trace.

---

# Terminal UX rules

## TTY gating (be precise)

- Prompts: only if **stdin is a TTY**.
- Progress/spinners: only if **stderr is a TTY**.
- Paging: only if **stdout is a TTY**.
- If `CI` is set, default to non-interactive behavior (no prompts; no spinners), unless explicitly forced.

## Color

- Default: enable color only when the relevant stream is a TTY (check stdout/stderr separately).
- Disable color when:
  - `NO_COLOR` is set (non-empty)
  - `TERM=dumb`
  - `--no-color`
- Allow forcing color via `FORCE_COLOR`.
- In `--plain`, disable color regardless.

## Progress

- Never emit animations to non-TTY output.
- Provide `--no-progress`.
- Put progress on stderr so stdout stays pipe-friendly.

## Paging

- If stdout is a TTY and output is long, optionally page (e.g., `less -FIRX`).
- Never page when stdout is not a TTY.

## Accessibility & i18n

- High-contrast, monochrome-friendly output.
- Avoid emoji / heavy ASCII art; if you use symbols, provide an opt-out.
- English fallback; if localized, keep error codes stable.

---

# Reliability & performance

- Fast startup; lazy-load heavy dependencies.
- Avoid network calls unless requested/required; show a short "doing X…" message if it may take time.
- Networked operations:
  - sensible default timeout; configurable via `--timeout`
  - retries with backoff for transient errors (document retry policy)
- Parallelism:
  - `--parallel N`
  - deterministic output ordering when possible
  - provide `--fail-fast` and/or `--keep-going` for batch ops
- Idempotence: repeated runs converge safely.

---

# Security & privacy

- Never print secrets by default.
- Mask sensitive values in debug output; add `--redact` to be extra conservative.
- Do not accept secrets via flags (leaks to shell history / process list).
  Prefer:
- `--token-file PATH`
- stdin (`--token-stdin` or `--token-file -`)
- secret stores/keychains when available

If env-based secrets are supported for ergonomics:

- make it opt-in
- redact in logs
- warn about shell history/process leakage where relevant

---

# Distribution & updates

- Prefer a single self-contained binary when feasible.
- Provide easy install paths (Homebrew/Scoop/AUR/pkg managers) and easy uninstall instructions.
- Reproducible releases; signed artifacts if possible.
- Optional self-update + gentle version checks; provide an opt-out env var.

## Version metadata

- `tool --version` prints just semver by default (script-friendly).
- Optionally: `tool version --json` prints build metadata (commit, build date, platform, feature flags).

---

# Analytics (if any)

- Never phone home without consent.
- If collecting metrics:
  - be explicit about what/why/how long
  - opt-in preferred; if opt-out, disclose loudly and make disabling trivial

---

# Testing & observability

- Golden tests for human output (run with `--no-color --plain`).
- Schema tests for `--json` and `--jsonl`.
- Invalid-input tests: fuzz flags/args and edge-case inputs.
- PTY/TTY integration tests:
  - prompts only when stdin is TTY
  - spinners only when stderr is TTY
  - color gating per stream
- Determinism controls for tests:
  - locale (`LC_ALL`)
  - timezone
  - terminal width if relevant
  - time mocking if output includes timestamps
- `--debug/--trace` logs to stderr (timestamps optional; document format).
- `--profile` prints timing breakdowns (stderr) without changing stdout.

---

# Deliverables checklist (agent must produce)

- [ ] Command tree + one-line descriptions
- [ ] Consistent flag kit applied across subcommands
- [ ] Help text with examples per command
- [ ] Stable `--json` schema + examples
- [ ] `--jsonl`/NDJSON guidance for streaming lists
- [ ] `--plain` when needed for line-based tooling
- [ ] Safety: `--dry-run`, TTY-gated prompts, `--yes`, `--no-input`, `--force` semantics documented
- [ ] Config: precedence + merge rules + `config init/show/paths/schema`
- [ ] Machine mode contract: stdout always parseable, even on errors
- [ ] TTY rules: color/progress/pager + `NO_COLOR/FORCE_COLOR` + `CI` behavior
- [ ] Completions generator
- [ ] Tests: golden + schema + invalid-input + PTY coverage
- [ ] Versioning and deprecation strategy documented

## Bonus power moves (optional)

- Context-aware help: suggest flags/values based on partial input.
- Interactive wizards (`init`, `login`) gated behind TTY; pure flags otherwise.
- Plugin system: discover commands from `$PATH` like `tool-foo` → `tool foo`.
- Per-subcommand schema: `tool <subcmd> schema` for output schema (in addition to `tool config schema`).
