# Rust CLI Template

This repository provides a batteries-included starting point for building cross-platform Rust CLIs. It is designed to be opinionated about developer experience while remaining easy to extend.

## Quick Start

- Install the latest stable Rust toolchain (`rustup default stable`).
- Fetch dependencies and verify the build:

  ```bash
  cargo test
  ```

- Run the CLI in place:

  ```bash
  cargo run -- run
  ```

- Scaffold a fresh project from this template:

  ```bash
  scripts/new-cli.sh my-cli
  ```

  ```powershell
  pwsh scripts/new-cli.ps1 my-cli
  ```

## Features

- `clap`-powered command interface with shared global flags (`-q`, `-v`, `--debug`, `--trace`, `--json`, `--yaml`, `--no-color`, `--dry-run`, `--yes`).
- `config`-based configuration loader that creates `$XDG_CONFIG_HOME/workspace-backend/config.toml` (or platform equivalents) on first run.
- Environment overrides using the `WORKSPACE_BACKEND__*` prefix; e.g. `WORKSPACE_BACKEND__LOGGING__LEVEL=debug`.
- Configurable data and state directories that honor XDG locations on Unix and the appropriate directories on Windows.
- Shell completion generation via `cargo run -- completions <shell>`.
- Logging, runtime limits, and diagnostics output ready to customize for your workflow.
- `scripts/new-cli.sh` to clone the template with a new crate name and paths.

## CLI Overview

```bash
cargo run -- --help
```

Key subcommands:

- `run [TASK]` – executes the primary workflow with optional profile overrides.
- `init` – creates or refreshes the config file (use `--force` or `--yes` to overwrite).
- `config show|path|reset` – inspects the effective configuration.
- `completions <shell>` – emits shell completions to stdout (`bash`, `zsh`, `fish`, `powershell`, `elvish`).

Global flags apply to every subcommand, enabling quiet mode, stacked verbosity (`-vv`), trace logging, dry runs, JSON/YAML output, color control, progress suppression, and timeouts.

## Configuration

- Default config path: `$XDG_CONFIG_HOME/workspace-backend/config.toml` (or `%APPDATA%\workspace-backend\config.toml` on Windows). Override with `--config <path>`.
- Sample configuration with inline comments is available at `examples/config.toml`.
- Data and state directories default to `$XDG_DATA_HOME/workspace-backend` and `$XDG_STATE_HOME/workspace-backend` (falling back to `~/.local/share` and `~/.local/state` when unset). Override inside the config file.
- Values support `~` expansion and environment variables (e.g. `$HOME/logs/app.log`).

## Development Workflow

- Format the codebase:

  ```bash
  cargo fmt
  ```

- Run the test suite:

  ```bash
  cargo test
  ```

- Recommended lint pass during active development:

  ```bash
  cargo clippy --all-targets --all-features
  ```

- Generate completions for your shell:

  ```bash
  cargo run -- completions bash > target/workspace-backend.bash
  ```

## Scaffold New Projects

- Run `scripts/new-cli.sh my-cli` (Unix shells) or `pwsh scripts/new-cli.ps1 my-cli` (Windows/PowerShell) to copy the template into `./my-cli` with all configuration files updated to the new crate name.
- Provide `--path /some/where` (or `-Path C:\work\my-cli`) to choose a different destination directory.
- Requirements: `python3` for the shell script, PowerShell 7 (`pwsh`) for the Windows script.

## Project Structure

- `src/main.rs` – CLI entry point, argument parsing, config loading, and command handlers.
- `examples/config.toml` – commented configuration template.
- `Cargo.toml` – dependencies and metadata for the template crate.

Feel free to fork this template and tailor the commands, config schema, or runtime behavior to your project's needs.
