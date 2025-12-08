# Important

- NEVER Use emojis in README files or commit messages!
- NEVER use emojis in the Code!
- Keep content for README files concise and to the point
- always document your changes

## Configuration and Storage

- When developing desktop applications (CLIs, TUIs or GUIs), we enable the user configuration via a config.toml file which is placed at $XDG_CONFIG_HOME/<app_name>/config.toml or ~/.config/<app_name>/config.toml
- This should be overwriteable through Environment variables and command line arguments with priority in the following order:

Command line.
Config file thats name is declared on the command line.
Environment vars
Local config file (if exists)
Global config file (if exists)

- On first run, if it doesn't exist, a config.toml file is created in the above directory using the default values
- In the repo, we provide a commented example config.toml under examples/
- create a json-schema for the config.toml and include a reference to the schema in the config.toml for LSP completions and checking
- The ~ symbol can be used within a config.toml file, along with environment variables like $XDG_CONFIG_HOME which are expanded into the correct paths
- App-data defaults to $XDG_DATA_HOME. If this is either not set or empty, a default equal to $HOME/.local/share (MacOS and Linux) should be used.
- Machine-specific state defaults to $XDG_STATE_HOME. If this is either not set or empty, a default equal to $HOME/.local/state (MacOS and Linux) should be used.
- The $XDG_STATE_HOME contains state data that should persist between (application) restarts, but that is not important or portable enough to the user that it should be stored in $XDG_DATA_HOME. It may contain:
  - actions history (logs, history, recently used files, …)
  - current state of the application that can be reused on a restart (view, layout, open files, undo history, …)

## Rust

always run cargo check after implementing changes. Fix any errors that occur.

use the config crate to implement app config via a config.toml file but make sure to use xdg compliant paths if the env variables are set, i.e. don't only rely on dirs::config_dir() but rather e.g. on std::env::var("XDG_CONFIG_HOME") and use the other as a fallback.

If you get dead code warnings, implement everything that is only stubbed out unless we do not need it. In that case, remove dead code.

## GO

always compile the code after implementing a change. Fix any erros that occur.

## Python

always use uv for package management, don't try to run python directly:

```bash
uv init # initiate a new python project if none exists

```

```bash
uv run <script_name.py> # Execute script

```

```bash
uv add <library_name> # install dependency

```

```bash
uv run python <python_code># if you ever want to run python directly

```

## JavaScript/TypeScript

always use bun for package management:

```bash
bun install # install packages

```

```bash
bun dev  # run server in dev mode
```
