# Quick Start

## Installation

```bash
cd custom-context-files
./install.sh              # Global install (~/.pi/agent/extensions/)
./install.sh --local      # Project-local install (.pi/extensions/)
```

Or manually:
```bash
cp -r custom-context-files ~/.pi/agent/extensions/
```

## Configuration

**Global config** (`~/.pi/agent/context.json`):
```json
{
  "contextFiles": [
    {
      "names": ["PERSONALITY.md", "PERSONA.md"],
      "optional": true
    },
    {
      "names": ["USERS.md", "USER.md"],
      "optional": true
    }
  ]
}
```

**Project config** (`.pi/context.json`):
```json
{
  "contextFiles": [
    {
      "names": ["PROJECT.md"],
      "optional": false
    }
  ]
}
```

## Usage

1. Create your context files in your project directory
2. Restart pi or run `/reload`
3. The extension will automatically load your files at startup

**Example notification:**
```
Loaded 2 custom context file(s): PERSONALITY.md, USERS.md
```

## Features

- ✓ Multiple alternative file names per entry (tried in order)
- ✓ Optional vs required files
- ✓ Global + project config merging
- ✓ Works alongside AGENTS.md/CLAUDE.md

See [README.md](README.md) for full documentation.
