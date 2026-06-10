---
name: oqto-browser
description: Controls browser instances for web testing, form filling, screenshots, and data extraction. Use when the user asks to browse a website, navigate web pages, fill forms, take screenshots, or test web applications. The agent can start its own browser session or control one started by the user.
allowed-tools: Bash(oqto-browser:*)
---

# Browser Control with oqto-browser

You can browse the web using `oqto-browser`. There are two ways to use it:

## Starting your own browser session

When you need to browse a website, just use `oqto-browser` with any session name. The daemon starts automatically:

```bash
oqto-browser open https://example.com          # Opens browser, navigates to URL
oqto-browser snapshot -i                        # List interactive elements
oqto-browser click @e1                          # Click element
oqto-browser screenshot /tmp/shot.png           # Take screenshot
```

The default session name is `default`. Use `--session <name>` for multiple parallel sessions.

## Controlling a user-started browser

When the user starts a browser from the UI and sends instructions to chat, they'll provide a session ID:

```bash
oqto-browser --session <SESSION_ID> snapshot -i
oqto-browser --session <SESSION_ID> open <url>
```

## Core workflow

1. Navigate: `oqto-browser open <url>`
2. Snapshot: `oqto-browser snapshot -i` (returns elements with refs like `@e1`, `@e2`)
3. Interact using refs from the snapshot
4. Re-snapshot after navigation or significant DOM changes
5. Screenshot to show the user what you see: `oqto-browser screenshot /tmp/shot.png`

## Commands

### Navigation
```bash
oqto-browser open <url>              # Navigate to URL
oqto-browser back                    # Go back
oqto-browser forward                 # Go forward
oqto-browser reload                  # Reload page
```

### Snapshot (page analysis)
```bash
oqto-browser snapshot                # Full accessibility tree
oqto-browser snapshot -i             # Interactive elements only (recommended)
```

### Interactions (use @refs from snapshot)
```bash
oqto-browser click @e1               # Click
oqto-browser fill @e2 "text"         # Clear and type
oqto-browser type @e2 "text"         # Type without clearing
oqto-browser press Enter             # Press key
oqto-browser hover @e1               # Hover
oqto-browser select @e1 "value"      # Select dropdown
oqto-browser scroll down 500         # Scroll page
oqto-browser scrollintoview @e1      # Scroll element into view
```

### Screenshots & Information
```bash
oqto-browser screenshot /tmp/shot.png  # Save screenshot (display with @/tmp/shot.png)
oqto-browser title                     # Get page title
oqto-browser url                       # Get current URL
oqto-browser console                   # View console messages
oqto-browser content                   # Get page HTML
```

### JavaScript
```bash
oqto-browser eval "document.title"     # Run JavaScript
```

### Wait
```bash
oqto-browser wait 2000                 # Wait milliseconds
```

## Example: Browse and extract info

```bash
oqto-browser open https://news.ycombinator.com
oqto-browser snapshot -i
# Output: link "Hacker News" [ref=e1], link "new" [ref=e2], ...

oqto-browser screenshot /tmp/hn.png
# Show to user: @/tmp/hn.png

oqto-browser click @e2   # Click "new" link
oqto-browser wait 1000
oqto-browser snapshot -i  # See new page
```

## Example: Form submission

```bash
oqto-browser open https://example.com/login
oqto-browser snapshot -i
# Output: textbox "Email" [ref=e1], textbox "Password" [ref=e2], button "Submit" [ref=e3]

oqto-browser fill @e1 "user@example.com"
oqto-browser fill @e2 "password123"
oqto-browser click @e3
oqto-browser wait 2000
oqto-browser snapshot -i   # Check result
```

## JSON output

Add `--json` for machine-readable output:
```bash
oqto-browser snapshot -i --json
```

## Closing

```bash
oqto-browser close                   # Close the browser session
```
