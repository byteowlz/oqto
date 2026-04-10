# oqtoctl a2ui - Interactive UI Prompts for Agents

Send interactive UI prompts to users and wait for their response. Uses the A2UI v0.8 protocol to render buttons, text inputs, media, and more in the Oqto web interface.

## Prerequisites

- `OQTO_SESSION_ID` environment variable set (automatically available in Pi agent context)
- Oqto server running and accessible (defaults to `http://localhost:8080`)

## Commands

### Input Components (Blocking)

#### Button Prompt

Ask user to click one of several buttons:

```bash
# Simple yes/no
RESULT=$(oqtoctl a2ui button -b "Yes,No")

# With prompt text
RESULT=$(oqtoctl a2ui button -p "Deploy to production?" -b "Deploy,Cancel")

# Multiple buttons
RESULT=$(oqtoctl a2ui button -p "Choose environment:" -b Staging -b Production -b Development
```

Returns the clicked button label (e.g., "Yes", "Deploy").

#### Text Input

Ask user for text input:

```bash
# Basic input
oqtoctl a2ui input "Enter the project name:"

# With placeholder and type
oqtoctl a2ui input "Enter API key:" --placeholder "sk-..." --input-type password

# Long text (textarea)
oqtoctl a2ui input "Describe the issue:" --input-type long

# Number input
oqtoctl a2ui input "Enter quantity:" --input-type number
```

Input types: `text` (default), `number`, `password`, `long`

#### Multiple Choice

Present a list of options:

```bash
# Single selection (radio buttons)
oqtoctl a2ui choice -p "Select language:" -c "Python,Rust,TypeScript,Go"

# Multi-select (checkboxes)
oqtoctl a2ui choice -p "Select features:" -c "Auth,Database,API,WebSocket" --multi
```

#### Checkbox (Boolean)

Single boolean toggle:

```bash
# Default unchecked
oqtoctl a2ui checkbox "Enable notifications"

# Default checked
oqtoctl a2ui checkbox "I agree to the terms" --checked
```

#### Slider (Numeric Range)

Numeric slider input:

```bash
# Basic slider (0-100)
oqtoctl a2ui slider -p "Select volume:"

# Custom range
oqtoctl a2ui slider -p "Set temperature:" --min 0 --max 40 --value 20
```

#### Date/Time Input

Date and/or time picker:

```bash
# Date only (default)
oqtoctl a2ui datetime -p "Select date:"

# Time only
oqtoctl a2ui datetime -p "Select time:" --date=false --time

# Date and time
oqtoctl a2ui datetime -p "Schedule for:" --time

# With initial value
oqtoctl a2ui datetime -p "Due date:" --value "2024-12-31"
```

### Display Components

#### Text Display (Non-blocking)

Show text message without waiting:

```bash
# Plain text
oqtoctl a2ui text "Processing started..."

# Styled text
oqtoctl a2ui text "Important Notice" --style h1
oqtoctl a2ui text "Please wait..." --style caption
```

Styles: `body` (default), `h1`, `h2`, `h3`, `h4`, `h5`, `caption`

#### Image

Display an image:

```bash
# Non-blocking (show and continue)
oqtoctl a2ui image "https://example.com/chart.png"

# Blocking (wait for confirmation)
oqtoctl a2ui image "https://example.com/result.png" --confirm

# With fit mode
oqtoctl a2ui image "https://example.com/photo.jpg" --fit cover --confirm
```

Fit modes: `contain` (default), `cover`, `fill`, `none`, `scale-down`

#### Video

Display a video player:

```bash
# Non-blocking
oqtoctl a2ui video "https://example.com/demo.mp4"

# Blocking (wait for confirmation)
oqtoctl a2ui video "https://example.com/tutorial.mp4" --confirm
```

#### Audio

Display an audio player:

```bash
# Basic audio
oqtoctl a2ui audio "https://example.com/podcast.mp3"

# With description
oqtoctl a2ui audio "https://example.com/song.mp3" --description "Background music"

# Blocking
oqtoctl a2ui audio "https://example.com/recording.mp3" --confirm
```

#### Tabs

Display tabbed content:

```bash
# Show tabbed information
oqtoctl a2ui tabs '[{"title":"Overview","content":"Project summary..."},{"title":"Details","content":"Technical specs..."}]'

# Blocking (wait for confirmation)
oqtoctl a2ui tabs '[{"title":"Tab1","content":"Content1"},{"title":"Tab2","content":"Content2"}]' --confirm
```

### Raw A2UI JSON

For advanced use cases, send raw A2UI messages:

```bash
# From stdin
echo '[{"surfaceUpdate":{"surfaceId":"custom","components":[...]}}]' | oqtoctl a2ui raw --blocking

# Inline JSON
oqtoctl a2ui raw --blocking '[{"surfaceUpdate":...}]'
```

## Global Options

| Option | Description |
|--------|-------------|
| `-s, --session <ID>` | Session ID (defaults to `OQTO_SESSION_ID` env var) |
| `-t, --timeout <SECS>` | Timeout in seconds (default: 300) |
| `--json` | Output machine-readable JSON |

## Output Format

### Standard Output (stdout)
- For buttons: The clicked button label
- For inputs: `submit` or `confirmed`
- For media with `--confirm`: `confirmed`

### Context Values (stderr)
Data-bound values are printed to stderr as `key=value`:
- `user_input=<text>` - from text input
- `selection=<value>` - from choice/checkbox
- `slider_value=<number>` - from slider
- `datetime_value=<iso8601>` - from datetime

### JSON Output
Use `--json` for structured output:
```json
{
  "action": {
    "name": "submit",
    "context": [{"key": "user_input", "value": "hello"}]
  }
}
```

## Examples

### Confirmation Dialog

```bash
#!/bin/bash
CONFIRM=$(oqtoctl a2ui button -p "Delete all data?" -b "Delete,Cancel")
if [ "$CONFIRM" = "Delete" ]; then
    rm -rf ./data
fi
```

### Collect User Input

```bash
#!/bin/bash
oqtoctl a2ui input "Enter commit message:" --placeholder "feat: ..." 2>/tmp/ctx
MESSAGE=$(grep "user_input=" /tmp/ctx | cut -d= -f2-)
git commit -m "$MESSAGE"
```

### Settings Form

```bash
#!/bin/bash
# Get values one at a time
ENABLE=$(oqtoctl a2ui checkbox "Enable feature X" 2>&1 | grep "checked=" | cut -d= -f2-)
LEVEL=$(oqtoctl a2ui slider -p "Set level:" --min 1 --max 10 2>&1 | grep "slider_value=" | cut -d= -f2-)
echo "Feature: $ENABLE, Level: $LEVEL"
```

### Show Progress with Media

```bash
#!/bin/bash
# Show processing status
oqtoctl a2ui text "Generating report..." --style h2

# ... do work ...

# Show result image and wait for acknowledgment
oqtoctl a2ui image "file:///tmp/report.png" --confirm
```

### Multi-select Options

```bash
#!/bin/bash
oqtoctl a2ui choice -p "Select components to install:" \
    -c "Core,Database,Cache,Queue,WebServer" --multi 2>/tmp/ctx

SELECTED=$(grep "selection=" /tmp/ctx | cut -d= -f2-)
echo "Installing: $SELECTED"
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `OQTO_SESSION_ID` | Default session ID for all commands |
| `OQTO_SERVER_URL` | Server URL (default: `http://localhost:8080`) |

## Error Handling

- Exit code 0: User responded successfully
- Exit code 1: Timeout, error, or user dismissed

```bash
if ! RESULT=$(oqtoctl a2ui button -b "OK" -t 30 2>&1); then
    echo "User did not respond in time"
    exit 1
fi
```

## A2UI Components Reference

| Command | A2UI Component | Interactive |
|---------|---------------|-------------|
| `button` | Button | Yes |
| `input` | TextField | Yes |
| `choice` | MultipleChoice | Yes |
| `checkbox` | CheckBox | Yes |
| `slider` | Slider | Yes |
| `datetime` | DateTimeInput | Yes |
| `text` | Text | No |
| `image` | Image | Optional |
| `video` | Video | Optional |
| `audio` | AudioPlayer | Optional |
| `tabs` | Tabs | Optional |
| `raw` | Any | Configurable |
