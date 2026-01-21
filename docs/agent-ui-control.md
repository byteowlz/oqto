# Agent UI Control

Agent UI control lets backend tools or `octoctl` drive the frontend via WebSocket
events. This is used for onboarding flows, spotlight tours, and agent-driven UI
navigation.

## WebSocket Event Types

- `ui.navigate` `{ path, replace }`
- `ui.session` `{ session_id, mode? }` (`mode`: `main` | `opencode` | `pi`)
- `ui.view` `{ view }`
- `ui.palette` `{ open }`
- `ui.palette_exec` `{ command, args? }`
- `ui.spotlight` `{ target?, title?, description?, action?, position?, active }`
- `ui.tour` `{ steps, start_index?, active }`
- `ui.sidebar` `{ collapsed? }`
- `ui.panel` `{ view?, collapsed? }`
- `ui.theme` `{ theme }`

## HTTP Endpoints (server)

All endpoints are POST under the main API router:

- `/ui/navigate`
- `/ui/session`
- `/ui/view`
- `/ui/palette`
- `/ui/palette/exec`
- `/ui/spotlight`
- `/ui/tour`
- `/ui/sidebar`
- `/ui/panel`
- `/ui/theme`

## `octoctl ui` Examples

```bash
octoctl ui navigate /sessions
octoctl ui session ses_123 --mode opencode
octoctl ui view files
octoctl ui palette --open true
octoctl ui palette-exec new_chat
octoctl ui spotlight --target chat-input --title "Send a message"
octoctl ui tour --steps '[{"target":"sidebar","title":"Sidebar"},{"target":"chat-input","title":"Chat input"}]'
octoctl ui sidebar --collapsed true
octoctl ui panel --view terminal
octoctl ui theme dark
```

## Spotlight Targets

The UI exposes these `data-spotlight` targets:

- `sidebar`
- `session-list`
- `file-tree`
- `todo-list`
- `terminal`
- `canvas`
- `chat-input`
- `chat-timeline`
- `model-picker`
- `command-palette`
- `memory-view`
- `trx-view`

## Palette Commands

Supported `ui.palette_exec` commands:

- `new_chat`
- `toggle_theme`
- `set_theme` (args: `{ "theme": "light" | "dark" | "system" }`)
- `toggle_locale`
- `set_locale` (args: `{ "locale": "de" | "en" }`)
- `open_app` (args: `{ "appId": "<app-id>" }`)
- `select_session` (args: `{ "sessionId": "<session-id>" }`)
