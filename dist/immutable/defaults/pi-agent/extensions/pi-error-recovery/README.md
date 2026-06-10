# pi-error-recovery

Automatically detects and recovers from common provider errors that pi's built-in retry logic does not handle.

## What it does

Pi has built-in retry for transient errors like rate limits (429) and server errors (500/502/503). However, some errors are recoverable client-side configuration issues that require changing a parameter before retrying:

- **Unsupported thinking/reasoning level** — e.g. `400 Unsupported value: 'minimal' is not supported with the 'gpt-5.3-codex-2026-02-24' model. Supported values are: 'none', 'low', 'medium', 'high', and 'xhigh'.`
- **Reasoning not supported at all** — Some models claim `reasoning: true` but the deployment rejects any reasoning parameter.
- **Unsupported image input** — Deployment rejects image content even though the model definition allows it.

This extension intercepts these errors, applies the appropriate fix, and retries automatically.

## How it works

1. **Detection** — Hooks `agent_end` and inspects the last assistant message for `stopReason: "error"`.
2. **Analysis** — Matches error text against known recoverable patterns.
3. **Fix** — For unsupported thinking levels, parses the supported values from the error and adjusts the thinking level. For unsupported reasoning, disables it.
4. **Retry** — Sends a `"continue"` follow-up message to re-trigger the turn with the fixed parameters.
5. **Context filtering** — Error assistant messages are filtered out of the next LLM context so the model isn't confused by them.
6. **Learning** — Records model-specific restrictions in memory so future requests can be patched preemptively via `before_provider_request`.

## Installation

Copy or symlink this directory into your pi extensions location:

```bash
# Global
ln -s $(pwd)/pi-error-recovery ~/.pi/agent/extensions/pi-error-recovery

# Project-local
ln -s $(pwd)/pi-error-recovery .pi/extensions/pi-error-recovery
```

Then run `pi` or use `/reload` to load the extension.

## Configuration

Optional `pi-error-recovery.json` (searched in cwd, `.pi/`, or `~/.pi/agent/`):

```json
{
  "enabled": true,
  "maxRetries": 3,
  "retryMessage": "continue",
  "debug": false,
  "handlers": {
    "thinkingLevel": true,
    "imageInput": true,
    "genericParameter": true
  }
}
```

| Option | Default | Description |
|--------|---------|-------------|
| `enabled` | `true` | Master switch |
| `maxRetries` | `3` | Maximum recovery attempts per error type before giving up |
| `retryMessage` | `"continue"` | Message sent to trigger the retry turn |
| `debug` | `false` | Show verbose notifications |
| `handlers.thinkingLevel` | `true` | Enable thinking/reasoning level recovery |
| `handlers.imageInput` | `true` | Enable image stripping recovery |
| `handlers.genericParameter` | `true` | Reserved for future generic parameter recovery |

## Commands

- `/error-recovery` or `/error-recovery status` — Show current state, learned restrictions, and handler status
- `/error-recovery reset` — Clear recovery state and learned model restrictions
- `/error-recovery debug` — List all learned model restrictions

## Example recovery flow

```
User: analyze this code
[pi sends request with thinkingLevel: "minimal"]
Provider: 400 Unsupported value: 'minimal' is not supported...
[extension detects error, sets thinkingLevel to "low"]
[extension sends "continue" follow-up]
[pi retries with corrected parameters]
```
