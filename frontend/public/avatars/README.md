# System Avatars

Default avatars that ship with oqto. Used for personas/agents.

## Specifications

- **Format**: PNG with transparency, or WebP
- **Size**: 256x256 pixels (1:1 aspect ratio)
- **Style**: Consistent visual style (TBD - illustrated, 3D, abstract)

## Avatar Types

System avatars for common agent roles:

- `default.png` - Generic/fallback avatar
- `developer.png` - Coding/development agent
- `researcher.png` - Research/analysis agent
- `writer.png` - Writing/documentation agent
- `analyst.png` - Data analysis agent
- `creative.png` - Creative/design agent
- `assistant.png` - General assistant

## Usage in persona.toml

```toml
# System avatar (absolute path from web root)
avatar = "/avatars/developer.png"

# Custom avatar (relative to persona directory)
avatar = "my-avatar.png"

# User-generated avatar (stored in ~/oqto/avatars/)
avatar = "user://generated-abc123.png"
```

## User Avatars

User-uploaded or generated avatars are stored in:
- `~/oqto/avatars/` (local mode)
- Container volume (container mode)

Same specifications apply (256x256 PNG/WebP).
