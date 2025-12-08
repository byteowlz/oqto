# AGENTS.md

Guidance for coding agents working on this Rust CLI template.

## Core Principles

- **Never publish** artifacts to public registries without explicit user approval.
- We favor clean refactors over backwards compatibility; update existing code in place (no `FooV2` suffixes).
- Target Windows 11, Linux, and macOS 14+ with the same behavior; no legacy OS shims.
- Keep file headers minimal—no author or timestamp banners.

## Rust Workflow

- Follow Clippy best practices: collapse trivial `if`s, inline `format!` arguments, and prefer method references over redundant closures.
- When tests compare structures, assert on the full value instead of individual fields.
- Run `cargo fmt` after code changes and `cargo test` for the touched crate. Invoke broader test or lint commands only if the user asks.

## CLI Expectations

- Prefer subcommands for verbs and keep outputs quiet/verbose via standard flags (`-q`, chainable `-v`, `--debug`, `--trace`).
- Support machine-readable modes via `--json/--yaml` and honor NO_COLOR/FORCE_COLOR.
- Offer `--dry-run`, `--yes/--force`, `--no-progress`, `--timeout`, and `--parallel` when operations warrant them.
- Generate help quickly (`-h/--help`) and provide shell completions off the same Clap definitions.

## Configuration & Storage

- Use XDG directories when available: config at `$XDG_CONFIG_HOME/<app>/config.toml`, data at `$XDG_DATA_HOME/<app>`, state at `$XDG_STATE_HOME/<app>` with sensible fallbacks (e.g., `~/.config`).
- Expand `~` and environment variables in config paths.
- Ship a commented example under `examples/`, create a default config on first run, and load overrides via the `config` crate.

## House Rules

- Do exactly what the user asks—no unsolicited files or docs.
- Keep README updates concise, emoji-free, and only when requested.
- Never commit secrets or sensitive paths; scrub logs before surfacing them.
