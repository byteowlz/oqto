# Onboarding System Design

Epic: `oqto-thhx` - Onboarding & Agent UI Control

## Vision

Progressive onboarding with wizard-driven setup and spotlight-based tutorials. New users see a minimal UI that expands as they learn, guided by step-by-step wizard forms and optional spotlight overlays.

**Setup is split into two layers:**
1. **System setup** (`setup.sh`) -- installs packages, tools, SearXNG, systemd services
2. **Config wizard** (`oqto-setup wizard` CLI or `/setup` web wizard) -- deployment mode, LLM provider, API keys, workspace, admin user

The web onboarding (provider, profile, personality) uses simple multi-step wizard forms, not agent-driven A2UI conversations.

## Onboarding Flow

```
[New User]
    |
    v
[Language Selection] -----> /godmode skips everything
    |                       Ctrl+Shift+G
    v
[Provider Setup] ---------> Skip if admin pre-configured EAVS
    |
    v
[Profile Setup] ----------> Fill USER.md via A2UI conversation
    |
    v
[Personality Setup] ------> Name assistant, customize PERSONALITY.md
    |
    v
[Progressive Tutorial]
    |-- Unlock: Main Chat (default)
    |-- Unlock: Sidebar (after first message)
    |-- Unlock: Todo list
    |-- Unlock: File tree
    |-- Unlock: Canvas
    |-- Unlock: Memory
    |-- Unlock: TRX view
    |-- Unlock: Projects (create first workspace)
    |-- Unlock: Terminal (technical users only)
    |-- Unlock: Model picker (power users)
    v
[Full UI Unlocked]
```

## Language Selection

### Animated Word Cloud with CRT Shader

Visual concept: floating, glitching words in a retro CRT aesthetic.

```
+--------------------------------------------------+
|     ░░ CLICK ░░    ░░ KLICK ░░                   |
|  ░░ CLIQUEZ ░░        ░░ KLIK ░░                 |
|       ░░ CLICCA ░░  ░░ HAZ CLIC ░░               |
|   ░░ KLIKNIJ ░░       ░░ KLIKK ░░                |
|        ░░ ...floating, glitching...░░            |
+--------------------------------------------------+
       [CRT scanlines + chromatic aberration]
```

Implementation:
- Three.js or pure CSS animations for word cloud
- WebGL post-processing for CRT effect (scanlines, flicker, chromatic aberration)
- Each word is a clickable button
- Click triggers language selection and fade transition to chat

### Multi-lingual Support

For polyglot users:
- Store `languages: string[]` in USER.md
- Primary language determines AGENTS.md variant
- Agent can switch languages based on context or explicit request
- Future: auto-detect from input text

### i18n AGENTS.md

Prepare agent instructions in multiple languages:
```
~/.local/share/oqto/main-chat/default/
  AGENTS.md          <- symlink or copy
  AGENTS.en.md
  AGENTS.de.md
  AGENTS.es.md
  AGENTS.fr.md
  AGENTS.pl.md
  ...
```

Alternative: dynamic injection of language-specific system prompt section at runtime.

## Provider Setup

Three scenarios:

| Scenario | Behavior |
|----------|----------|
| Admin pre-configured EAVS + default keys | Skip entirely |
| EAVS enabled, no user keys | A2UI wizard to add API keys |
| No EAVS (local mode) | Show provider selection, store keys locally |

### A2UI Wizard Flow

1. **Provider selection** (MultipleChoice)
   - Anthropic, OpenAI, Google, etc.
   
2. **API key input** (TextField with obscured type)
   - Paste your API key
   
3. **Connection test** (Button + status indicator)
   - Send "Hello" to verify
   
4. **Success** -> proceed to profile setup

## Profile & Personality Setup

Agent-driven conversation using A2UI components.

### USER.md Fields

```markdown
# About the User

## Basics
- **Name**: [TextField]
- **What to call them**: [TextField] 
- **Timezone**: [MultipleChoice or auto-detect]

## Context
- Current projects, interests (built over time)

## Communication Style
- [MultipleChoice: concise/detailed, formal/casual]
```

### PERSONALITY.md Customization

```markdown
# Assistant Personality

## Identity
- **Name**: [TextField - "What should I call myself?"]
- **Signature**: [TextField - optional emoji/phrase]

## Vibe
- [MultipleChoice: professional, friendly, playful, minimal]
```

## Progressive UI Unlock

### Unlock State Model

```typescript
interface OnboardingState {
  completed: boolean;
  stage: 'language' | 'provider' | 'profile' | 'personality' | 'tutorial' | 'complete';
  language: string;
  languages: string[];  // for polyglots
  unlocked_components: {
    sidebar: boolean;
    todo_list: boolean;
    file_tree: boolean;
    canvas: boolean;
    memory: boolean;
    trx: boolean;
    terminal: boolean;
    model_picker: boolean;
    projects: boolean;
  };
  user_level: 'beginner' | 'intermediate' | 'technical';
}
```

### Unlock Triggers

| Component | Trigger |
|-----------|---------|
| Sidebar | After first chat message |
| Todo list | Agent introduces during tutorial |
| File tree | Agent shows first file |
| Canvas | Agent shares an image |
| Memory | Agent explains memory system |
| TRX | Agent creates first issue |
| Projects | Tutorial creates first workspace |
| Terminal | Technical user detection |
| Model picker | User expresses interest in models |

## Agent UI Control

The agent needs programmatic control over the UI for guided onboarding.

### CLI Commands (oqtoctl ui)

```bash
# Navigation
oqtoctl ui navigate --app sessions|settings|admin|projects
oqtoctl ui session --id <session-id>
oqtoctl ui view --tab chat|files|tasks|memories|canvas|terminal|settings

# Command palette
oqtoctl ui palette open [--search "query"] [--auto-select "item"]
oqtoctl ui palette close
oqtoctl ui palette exec "New Chat"

# Spotlight
oqtoctl ui spotlight --target "sidebar" --message "Navigation here" [--position right] [--pulse]
oqtoctl ui spotlight clear

# Tour (sequential spotlights)
oqtoctl ui tour --steps '[
  {"target": "chat-input", "message": "Talk to me here"},
  {"target": "sidebar", "message": "Your sessions"},
  {"target": "file-tree", "message": "Browse files"}
]'

# Panels
oqtoctl ui sidebar toggle|open|close
oqtoctl ui panel --name canvas|terminal|preview --expand|--collapse

# Theme
oqtoctl ui theme dark|light|toggle
```

### WebSocket Events

```typescript
type UIControlEvent =
  | { type: 'ui.navigate'; app: string }
  | { type: 'ui.session'; session_id: string }
  | { type: 'ui.view'; view: string }
  | { type: 'ui.palette'; action: 'open' | 'close'; search?: string }
  | { type: 'ui.palette_exec'; command: string }
  | { type: 'ui.spotlight'; target: string; message: string; position?: string; pulse?: boolean }
  | { type: 'ui.spotlight_clear' }
  | { type: 'ui.tour'; steps: TourStep[] }
  | { type: 'ui.sidebar'; action: 'toggle' | 'open' | 'close' }
  | { type: 'ui.panel'; name: string; expanded: boolean }
  | { type: 'ui.theme'; theme: 'light' | 'dark' }
```

## Spotlight System

### Visual Design

```
+--------------------------------------------------+
|░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░|
|░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░|
|░░░░░░░░░░+------------------+░░░░░░░░░░░░░░░░░░░░|
|░░░░░░░░░░|                  |░░░░░░░░░░░░░░░░░░░░|
|░░░░░░░░░░|   [Target UI]    |<-- "This is where  |
|░░░░░░░░░░|    (spotlight)   |    your files      |
|░░░░░░░░░░|                  |    appear!"        |
|░░░░░░░░░░+------------------+░░░░░░░░░░░░░░░░░░░░|
|░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░|
+--------------------------------------------------+
```

### Implementation

1. **data-spotlight attributes** on UI elements:
   ```tsx
   <aside data-spotlight="sidebar">
   <div data-spotlight="file-tree">
   <textarea data-spotlight="chat-input">
   ```

2. **SpotlightOverlay component**:
   - SVG mask with cutout for target element
   - Positioned tooltip with message
   - Optional pulse animation
   - Click-to-dismiss or auto-advance

3. **Tour mode**:
   - Array of steps with targets and messages
   - Progress indicator
   - Skip button
   - Auto-advance on timeout or user action

## Technical User Detection

Subtle detection without making it obvious:

### Method 1: Profile Questions
During setup, ask about work/interests. Keywords like "developer", "sysadmin", "devops" indicate technical user.

### Method 2: A2UI Choice
Present options that reveal preference:
```
Agent: "I can show you what's in a directory..."
[A2UI MultipleChoice]
  - "Show me visually"
  - "Run `ls -la`"
```
If they pick the command -> technical user.

### Method 3: Input Detection
If user types something that looks like a shell command in chat:
```
User: ls -la
Agent: "Looks like you know your way around a terminal! Want me to unlock it for you?"
```

### Method 4: Direct Question
```
Agent: "Some people prefer visual interfaces, others like direct terminal access. Which sounds like you?"
```

## Godmode

For power users and developers who want to skip onboarding:

### Activation Methods

1. **Slash command**: `/godmode` in chat
2. **Keyboard shortcut**: Ctrl+Shift+G during onboarding
3. **URL parameter**: `/onboarding?godmode=true`

### Effect

- Marks onboarding complete
- Unlocks all UI components
- Sets user_level to 'technical'
- Redirects to full UI

## Task Breakdown

See trx epic `oqto-thhx` for full task list.

### P1 (Core Infrastructure)
- oqto-thhx.1: Onboarding state model
- oqto-thhx.2: Onboarding API endpoints
- oqto-thhx.3: UIControlContext
- oqto-thhx.4: WebSocket ui.* events
- oqto-thhx.5: oqtoctl ui CLI
- oqto-thhx.6: Spotlight overlay
- oqto-thhx.7: data-spotlight attributes

### P2 (Onboarding Flow)
- oqto-thhx.8: Tour mode
- oqto-thhx.9: Language word cloud + CRT shader
- oqto-thhx.10: Onboarding route
- oqto-thhx.11: Godmode
- oqto-thhx.12: Progressive unlock
- oqto-thhx.13: i18n AGENTS.md
- oqto-thhx.14: Provider setup wizard
- oqto-thhx.15: Profile/personality setup
- oqto-thhx.16: Tutorial script

### P3 (Polish)
- oqto-thhx.17: Technical user detection
- oqto-thhx.18: Multi-lingual support

## Dependencies

- **oqto-wmrf.5** (MCP Tool: a2ui_surface) - Not needed, use `oqtoctl a2ui` CLI instead
- **EAVS** - For provider proxy, needs testing as single point of access

## Open Questions

1. **EAVS as sole provider proxy**: What if users want direct API keys without EAVS? Support both modes?

2. **Onboarding persistence**: Backend state (preferred) vs localStorage? What if user clears browser?

3. **Multi-device**: If user completes onboarding on desktop, what happens on mobile?

4. **Admin override**: Should admins be able to set default onboarding state for new users?

5. **Re-onboarding**: Can users restart the tutorial? Reset their profile?
