# A2UI Examples

Interactive UI surfaces that agents can send to users.

## Basic Examples

### Simple Confirmation

```bash
octoctl a2ui button "Deploy to production?" --options "Deploy,Cancel"
```

```json
{"type":"text","content":"Ready to deploy **v2.3.1** to production."}
{"type":"button","id":"deploy","label":"Deploy","action":"deploy","variant":"primary"}
{"type":"button","id":"cancel","label":"Cancel","action":"cancel","variant":"secondary"}
```

### Text Input

```bash
octoctl a2ui input "Enter commit message" --type text
```

```json
{"type":"text","content":"Describe your changes:"}
{"type":"textField","id":"message","placeholder":"feat: add new feature...","value":{"path":"/message"}}
{"type":"button","id":"submit","label":"Commit","action":"commit"}
```

### Multiple Choice (Single Select)

```bash
octoctl a2ui choice "Select environment" --options "Development,Staging,Production"
```

```json
{"type":"text","content":"Which environment?"}
{"type":"multipleChoice","id":"env","options":[{"value":"dev","label":"Development"},{"value":"staging","label":"Staging"},{"value":"prod","label":"Production"}],"maxAllowedSelections":1,"selections":{"path":"/selectedEnv"}}
{"type":"button","id":"select","label":"Continue","action":"select_env"}
```

### Toggle/Checkbox

```bash
octoctl a2ui checkbox "Enable verbose logging" --default false
```

```json
{"type":"checkBox","id":"verbose","label":"Enable verbose logging","value":{"path":"/verbose"}}
{"type":"button","id":"apply","label":"Apply Settings","action":"apply"}
```

### Slider

```bash
octoctl a2ui slider "Concurrency level" --min 1 --max 16 --default 4
```

```json
{"type":"text","content":"How many parallel workers?"}
{"type":"slider","id":"concurrency","min":1,"max":16,"step":1,"value":{"path":"/concurrency"}}
{"type":"button","id":"set","label":"Set Concurrency","action":"set_concurrency"}
```

---

## Complex Examples

### Code Review Form

Agent asks for review decision with comments:

```json
{"type":"text","content":"## Code Review: PR #127\n\n**Title:** Add user authentication\n**Author:** @alice\n**Files changed:** 12"}
{"type":"divider"}
{"type":"text","content":"### Your Review"}
{"type":"multipleChoice","id":"decision","options":[{"value":"approve","label":"Approve"},{"value":"request_changes","label":"Request Changes"},{"value":"comment","label":"Comment Only"}],"maxAllowedSelections":1,"selections":{"path":"/decision"}}
{"type":"textField","id":"comment","placeholder":"Add your review comments...","multiline":true,"value":{"path":"/comment"}}
{"type":"row","children":["btn_submit","btn_skip"]}
{"type":"button","id":"btn_submit","label":"Submit Review","action":"submit_review","variant":"primary"}
{"type":"button","id":"btn_skip","label":"Skip","action":"skip","variant":"ghost"}
```

### Deployment Configuration

Agent presents deployment options:

```json
{"type":"text","content":"## Deploy Configuration\n\nConfigure your deployment settings:"}
{"type":"card","id":"env_card","title":"Environment","children":["env_choice"]}
{"type":"multipleChoice","id":"env_choice","options":[{"value":"dev","label":"Development"},{"value":"staging","label":"Staging"},{"value":"prod","label":"Production"}],"maxAllowedSelections":1,"selections":{"path":"/env"}}
{"type":"card","id":"opts_card","title":"Options","children":["opt_migrate","opt_backup","opt_notify"]}
{"type":"checkBox","id":"opt_migrate","label":"Run database migrations","value":{"path":"/options/migrate"}}
{"type":"checkBox","id":"opt_backup","label":"Create backup before deploy","value":{"path":"/options/backup"}}
{"type":"checkBox","id":"opt_notify","label":"Notify team on Slack","value":{"path":"/options/notify"}}
{"type":"card","id":"replicas_card","title":"Scaling","children":["replicas_slider","replicas_text"]}
{"type":"slider","id":"replicas_slider","min":1,"max":10,"step":1,"value":{"path":"/replicas"}}
{"type":"text","id":"replicas_text","content":"Replicas: will scale to selected count"}
{"type":"divider"}
{"type":"row","children":["btn_deploy","btn_cancel"]}
{"type":"button","id":"btn_deploy","label":"Deploy Now","action":"deploy","variant":"primary"}
{"type":"button","id":"btn_cancel","label":"Cancel","action":"cancel","variant":"secondary"}
```

### File Conflict Resolution

Agent shows merge conflict options:

```json
{"type":"text","content":"## Merge Conflict\n\n`src/config.ts` has conflicts:"}
{"type":"card","id":"conflict","title":"Conflicting Changes","children":["current","incoming"]}
{"type":"text","id":"current","content":"**Current (HEAD):**\n```ts\nconst PORT = 3000;\n```"}
{"type":"text","id":"incoming","content":"**Incoming (feature-branch):**\n```ts\nconst PORT = process.env.PORT || 8080;\n```"}
{"type":"divider"}
{"type":"text","content":"How do you want to resolve this?"}
{"type":"multipleChoice","id":"resolution","options":[{"value":"current","label":"Keep current (HEAD)"},{"value":"incoming","label":"Accept incoming"},{"value":"both","label":"Keep both"},{"value":"manual","label":"Edit manually"}],"maxAllowedSelections":1,"selections":{"path":"/resolution"}}
{"type":"button","id":"resolve","label":"Resolve Conflict","action":"resolve"}
```

### Task Prioritization

Agent asks user to prioritize tasks:

```json
{"type":"text","content":"## Sprint Planning\n\nSelect tasks for this sprint (max 5):"}
{"type":"multipleChoice","id":"tasks","options":[{"value":"auth","label":"Implement OAuth login"},{"value":"api","label":"Add REST API endpoints"},{"value":"tests","label":"Write unit tests"},{"value":"docs","label":"Update documentation"},{"value":"perf","label":"Performance optimization"},{"value":"bugs","label":"Fix reported bugs"},{"value":"ui","label":"UI polish and fixes"}],"maxAllowedSelections":5,"selections":{"path":"/selectedTasks"}}
{"type":"divider"}
{"type":"text","content":"Set sprint duration:"}
{"type":"slider","id":"duration","min":1,"max":4,"step":1,"value":{"path":"/sprintWeeks"}}
{"type":"text","content":"Sprint duration: weeks"}
{"type":"row","children":["btn_create","btn_cancel"]}
{"type":"button","id":"btn_create","label":"Create Sprint","action":"create_sprint","variant":"primary"}
{"type":"button","id":"btn_cancel","label":"Cancel","action":"cancel"}
```

### Error Report Form

Agent collects error details:

```json
{"type":"text","content":"## Report an Issue\n\nHelp us understand what went wrong:"}
{"type":"textField","id":"title","label":"Title","placeholder":"Brief description of the issue","value":{"path":"/title"}}
{"type":"multipleChoice","id":"severity","options":[{"value":"critical","label":"Critical - System down"},{"value":"high","label":"High - Major feature broken"},{"value":"medium","label":"Medium - Feature partially working"},{"value":"low","label":"Low - Minor inconvenience"}],"maxAllowedSelections":1,"selections":{"path":"/severity"}}
{"type":"textField","id":"steps","label":"Steps to Reproduce","placeholder":"1. Go to...\n2. Click on...\n3. See error","multiline":true,"value":{"path":"/steps"}}
{"type":"textField","id":"expected","label":"Expected Behavior","placeholder":"What should have happened?","multiline":true,"value":{"path":"/expected"}}
{"type":"checkBox","id":"include_logs","label":"Include system logs","value":{"path":"/includeLogs"}}
{"type":"button","id":"submit","label":"Submit Report","action":"submit_report","variant":"primary"}
```

### Settings Panel

Agent presents configuration options:

```json
{"type":"text","content":"## Agent Settings"}
{"type":"tabs","id":"settings_tabs","tabs":[{"id":"general","label":"General","children":["model_choice","temp_slider"]},{"id":"behavior","label":"Behavior","children":["auto_compact","verbose"]},{"id":"limits","label":"Limits","children":["max_tokens","timeout"]}]}
{"type":"text","id":"model_choice_label","content":"### Model"}
{"type":"multipleChoice","id":"model_choice","options":[{"value":"claude-sonnet","label":"Claude 3.5 Sonnet"},{"value":"claude-opus","label":"Claude 3 Opus"},{"value":"gpt-4","label":"GPT-4o"}],"maxAllowedSelections":1,"selections":{"path":"/model"}}
{"type":"text","id":"temp_label","content":"### Temperature"}
{"type":"slider","id":"temp_slider","min":0,"max":1,"step":0.1,"value":{"path":"/temperature"}}
{"type":"checkBox","id":"auto_compact","label":"Auto-compact when context full","value":{"path":"/autoCompact"}}
{"type":"checkBox","id":"verbose","label":"Verbose tool output","value":{"path":"/verbose"}}
{"type":"text","id":"tokens_label","content":"### Max Tokens"}
{"type":"slider","id":"max_tokens","min":1000,"max":100000,"step":1000,"value":{"path":"/maxTokens"}}
{"type":"text","id":"timeout_label","content":"### Request Timeout (seconds)"}
{"type":"slider","id":"timeout","min":30,"max":300,"step":30,"value":{"path":"/timeout"}}
{"type":"button","id":"save","label":"Save Settings","action":"save_settings","variant":"primary"}
```

### Media Display

Agent shows images/video:

```json
{"type":"text","content":"## Generated Assets\n\nHere are the images I created:"}
{"type":"row","children":["img1","img2"]}
{"type":"image","id":"img1","src":"/outputs/hero-dark.png","alt":"Hero section dark mode"}
{"type":"image","id":"img2","src":"/outputs/hero-light.png","alt":"Hero section light mode"}
{"type":"divider"}
{"type":"text","content":"Preview video:"}
{"type":"video","id":"preview","src":"/outputs/demo.mp4","autoplay":false,"controls":true}
{"type":"row","children":["btn_download","btn_regenerate"]}
{"type":"button","id":"btn_download","label":"Download All","action":"download"}
{"type":"button","id":"btn_regenerate","label":"Regenerate","action":"regenerate","variant":"secondary"}
```

### Date/Time Selection

Agent asks for scheduling:

```json
{"type":"text","content":"## Schedule Deployment\n\nWhen should the deployment run?"}
{"type":"multipleChoice","id":"when","options":[{"value":"now","label":"Deploy immediately"},{"value":"scheduled","label":"Schedule for later"}],"maxAllowedSelections":1,"selections":{"path":"/deployWhen"}}
{"type":"dateTimeInput","id":"schedule_time","label":"Scheduled Time","value":{"path":"/scheduledTime"}}
{"type":"checkBox","id":"notify_before","label":"Notify me 15 minutes before","value":{"path":"/notifyBefore"}}
{"type":"button","id":"confirm","label":"Confirm Schedule","action":"schedule_deploy","variant":"primary"}
```

---

## Using with octoctl

### Send Raw JSON

```bash
octoctl a2ui raw '[
  {"type":"text","content":"Custom surface"},
  {"type":"button","id":"ok","label":"OK","action":"acknowledge"}
]'
```

### Environment Variables

```bash
export OCTO_SESSION_ID=main-chat
export OCTO_API_URL=http://localhost:8080

octoctl a2ui button "Continue?" --options "Yes,No"
```

### Blocking Mode (wait for response)

```bash
# This will block until user clicks a button
RESPONSE=$(octoctl a2ui button "Approve?" --options "Approve,Reject" --blocking --timeout 120)
echo "User selected: $RESPONSE"
```

---

## Data Binding

Values can be bound to paths in the data model:

```json
{"type":"textField","id":"name","value":{"path":"/user/name"}}
```

When the user types, the value is stored at `/user/name` in the context.

When submitting an action, the entire data model is sent back:

```json
{
  "action": "submit",
  "context": {
    "user": {
      "name": "Alice"
    }
  }
}
```

---

## Component Reference

| Component | Purpose |
|-----------|---------|
| `text` | Markdown text content |
| `button` | Clickable action button |
| `textField` | Single or multi-line text input |
| `checkBox` | Toggle switch |
| `multipleChoice` | Radio buttons or checkboxes |
| `slider` | Numeric range input |
| `dateTimeInput` | Date/time picker |
| `image` | Display image |
| `video` | Video player |
| `audioPlayer` | Audio player |
| `divider` | Horizontal separator |
| `card` | Grouped content with title |
| `tabs` | Tabbed content sections |
| `row` | Horizontal layout |
| `column` | Vertical layout |
| `list` | Repeated items |
| `modal` | Overlay dialog |
