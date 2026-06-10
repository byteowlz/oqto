# Custom Context Files Extension

Automatically load additional context files beyond `AGENTS.md`/`CLAUDE.md` with configurable file names and precedence rules.

## Features

- Load custom context files at agent startup
- Global configuration via `~/.pi/agent/context.json`
- Project-specific configuration via `.pi/context.json`
- Automatic merging of global and project configs (project takes precedence)
- Support for multiple alternative file names per entry (e.g., `USER.md` or `USERS.md`)
- Mark files as optional or required
- Silent handling of missing optional files
- Warnings for missing required files

## Installation

### Option 1: Manual Installation

```bash
# Copy to global extensions
cp -r custom-context-files ~/.pi/agent/extensions/

# Or copy to project extensions
cp -r custom-context-files .pi/extensions/
```

### Option 2: Via `pi install`

```bash
# From git
pi install git:github.com/yourusername/pi-agent-extensions#main

# From local path
pi install path:../pi-agent-extensions/custom-context-files
```

## Configuration

### Global Config

Create `~/.pi/agent/context.json`:

```json
{
  "contextFiles": [
    {
      "names": ["USERS.md", "USER.md"],
      "optional": true
    },
    {
      "names": ["PERSONALITY.md", "PERSONA.md"],
      "optional": false
    }
  ]
}
```

### Project Config

Create `.pi/context.json` in your project directory:

```json
{
  "contextFiles": [
    {
      "names": ["PROJECT.md", "README.md"],
      "optional": false
    },
    {
      "names": ["TEAM.md"],
      "optional": true
    }
  ]
}
```

### Merging Behavior

- Global and project configs are merged
- Project config entries are added after global entries
- Both configs' files are loaded

Example:
- `~/.pi/agent/context.json` defines `USERS.md` and `PERSONA.md`
- `.pi/context.json` defines `PROJECT.md`
- Result: All three files are loaded

## Configuration Options

Each `contextFiles` entry:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `names` | `string[]` | Yes | List of alternative file names (tried in order) |
| `optional` | `boolean` | No | If `true`, file is silently skipped if not found (default: `false`) |

### File Name Precedence

Files are tried in the order specified in `names`:

```json
{
  "names": ["USERS.md", "USER.md"]
}
```

- If `USERS.md` exists, it's loaded
- Otherwise, if `USER.md` exists, it's loaded
- If neither exists, behavior depends on `optional` flag

## Example Use Cases

### User Profiles

```json
{
  "contextFiles": [
    {
      "names": ["USERS.md", "USER.md", "TEAM.md"],
      "optional": true
    }
  ]
}
```

### Project Conventions

```json
{
  "contextFiles": [
    {
      "names": ["CONVENTIONS.md", "STYLE.md", "GUIDELINES.md"],
      "optional": false
    }
  ]
}
```

### Multi-File Setup

Global (`~/.pi/agent/context.json`):
```json
{
  "contextFiles": [
    {
      "names": ["PERSONALITY.md", "PERSONA.md"],
      "optional": true
    },
    {
      "names": ["GLOBAL-RULES.md"],
      "optional": true
    }
  ]
}
```

Project (`.pi/context.json`):
```json
{
  "contextFiles": [
    {
      "names": ["PROJECT.md"],
      "optional": false
    },
    {
      "names": ["TECH-STACK.md"],
      "optional": true
    }
  ]
}
```

Result: All matching files from both configs are loaded.

## Output

When files are loaded, you'll see a notification:

```
Loaded 3 custom context file(s): PERSONALITY.md, PROJECT.md, USERS.md
```

If a required file is missing:

```
Required context file not found: CONVENTIONS.md or STYLE.md or GUIDELINES.md
```

## How It Works

1. At agent startup, the extension reads both `~/.pi/agent/context.json` and `.pi/context.json`
2. Configs are merged (project entries appended after global entries)
3. For each entry, tries to load files in the specified order
4. Loaded files are appended to the system prompt with HTML-style comments
5. The agent sees all context in its system prompt

## File Format

Context files are standard Markdown. They'll be injected into the system prompt with delimiters:

```
<!-- USERS.md -->
Content of your USERS.md file here...

<!-- PERSONALITY.md -->
Content of your PERSONALITY.md file here...
```

## Troubleshooting

### Files not loading

1. Check that `context.json` is in the correct location (`~/.pi/agent/` or `.pi/`)
2. Validate JSON syntax (e.g., with `jq . ~/.pi/agent/context.json`)
3. Check that files exist relative to your current working directory
4. Run pi with `--verbose` to see startup messages

### Permission errors

Ensure files and directories are readable:

```bash
chmod +r ~/.pi/agent/context.json
chmod +r .pi/context.json
chmod +r YOUR_CONTEXT_FILES.md
```

### Conflicts with AGENTS.md

This extension works alongside `AGENTS.md`/`CLAUDE.md`. All files (built-in + custom) are concatenated into the final system prompt.

## License

MIT
