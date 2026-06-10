# Oqto Todos Extension

A Pi extension that provides todo management tools compatible with Oqto's frontend todo panel.

## Overview

This extension provides a drop-in replacement for OpenCode's `todowrite` and `todoread` tools. Todos created through these tools are automatically displayed in Oqto's right sidebar panel.

## Tools

### `todowrite`

Write/replace the entire todo list. This is the primary tool for task planning.

```json
{
  "todos": [
    {
      "content": "Implement authentication",
      "status": "in_progress",
      "priority": "high"
    },
    {
      "content": "Write unit tests",
      "status": "pending",
      "priority": "medium"
    }
  ]
}
```

### `todoread`

Read the current todo list with optional filtering.

```json
{
  "filter": {
    "status": "pending",
    "priority": "high"
  }
}
```

### `todo`

Unified tool for incremental todo operations:

- **add**: Add a new todo
- **update**: Update an existing todo by ID
- **remove**: Remove a todo by ID
- **list**: List all todos

```json
// Add
{ "action": "add", "content": "New task", "priority": "high" }

// Update
{ "action": "update", "id": "abc123", "status": "completed" }

// Remove
{ "action": "remove", "id": "abc123" }

// List
{ "action": "list" }
```

## Todo Structure

```typescript
interface TodoItem {
  id: string;       // Auto-generated if not provided
  content: string;  // Task description
  status: "pending" | "in_progress" | "completed" | "cancelled";
  priority: "high" | "medium" | "low";
}
```

## Frontend Integration

The Oqto frontend automatically parses `todowrite` tool calls and displays todos in the right sidebar panel. The frontend looks for tool calls with:
- Name containing "todo" or exactly "todowrite"/"todoread"
- Input containing a `todos` array with the expected structure

## Commands

- `/todos` - Display current todos in the notification area

## Configuration

Create `oqto-todos.json` in your project root, `.pi/` directory, or `~/.pi/agent/`:

```json
{
  "enabled": true,
  "debug": false,
  "sessionScoped": true,
  "storagePath": ".pi/todos",
  "tuiWidget": false
}
```

### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | boolean | `true` | Enable/disable the extension |
| `debug` | boolean | `false` | Enable debug logging |
| `sessionScoped` | boolean | `true` | Store todos per session (vs. shared) |
| `storagePath` | string | `.pi/todos` | Directory for todo storage |
| `tuiWidget` | boolean | `true` | Render persistent todo widget in Pi TUI (set to `false` for Oqto-only frontend usage) |

## Installation

### Global Installation

```bash
cp -r oqto-todos ~/.pi/agent/extensions/
```

### Project-local Installation

```bash
cp -r oqto-todos .pi/extensions/
```

### Development (Symlink)

```bash
ln -s $(pwd)/oqto-todos ~/.pi/agent/extensions/oqto-todos
```

## Storage

Todos are stored as JSON files in `.pi/todos/`:

- **Session-scoped**: `.pi/todos/<session-id>.json`
- **Shared**: `.pi/todos/todos.json`

## Compatibility

This extension is designed to be compatible with:
- OpenCode's `todowrite` and `todoread` tools
- Oqto's frontend todo panel
- Pi's extension system

The output format matches exactly what Oqto's frontend expects, ensuring seamless integration.
