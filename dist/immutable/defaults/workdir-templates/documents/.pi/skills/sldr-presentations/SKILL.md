---
name: sldr-presentations
description: |
  Create markdown-based presentations using sldr CLI. Use this skill when asked to:
  create slides, build presentations, manage presentation skeletons, or work with slidev-powered presentations.
  Triggers: "create a presentation", "make slides", "build a talk", "presentation about X", "sldr".
---

# sldr Presentations Skill

sldr is a CLI tool for creating modular, reusable markdown presentations powered by slidev.

## Key Concepts

- **Slides**: Individual markdown files in `~/sldr/slides/` (can be in subdirectories)
- **Skeletons**: TOML files in `~/sldr/skeletons/` that define which slides to include
- **Flavors**: Style definitions in `~/.config/sldr/flavors/` (colors, fonts, backgrounds)
- **Templates**: Slide templates in `~/.config/sldr/templates/` (layouts like cover, two-cols, code)

## Agent Workflow

### Creating a Presentation

1. Create slides using JSON input
2. Create a skeleton referencing those slides
3. Validate the skeleton
4. Build the presentation

### Step 1: Create Slides

Use `sldr slides create` with JSON input:

```bash
sldr slides create --file /tmp/slides.json --json
```

**JSON Schema** (write to file, then pass via `--file`):
```json
{
  "directory": "topic-name",
  "slides": [
    {
      "name": "slide-filename",
      "title": "Slide Title",
      "description": "Optional description",
      "layout": "default",
      "tags": ["tag1", "tag2"],
      "content": "# Heading\n\nMarkdown content here."
    }
  ]
}
```

**Available layouts**: `default`, `cover`, `two-cols`, `two-cols-header`, `image`, `image-left`, `image-right`, `center`, `quote`, `section`, `end`

**Flags**:
- `--file PATH`: Read JSON from file (recommended over stdin)
- `--json`: Output results as JSON
- `--dry-run`: Preview without creating files
- `--force`: Overwrite existing slides

### Step 2: Create Skeleton

Use `sldr skeleton create` with JSON input:

```bash
sldr skeleton create --file /tmp/skeleton.json --json
```

**JSON Schema**:
```json
{
  "name": "presentation-name",
  "title": "Presentation Title",
  "description": "Optional description",
  "slides": [
    "topic-name/slide-1",
    "topic-name/slide-2",
    "conclusion"
  ],
  "flavor": "default"
}
```

**Flags**: Same as slides create (`--file`, `--json`, `--dry-run`, `--force`)

### Step 3: Validate Skeleton

```bash
sldr skeleton validate presentation-name --json
```

Returns list of found/missing slides.

### Step 4: Build Presentation

```bash
sldr build presentation-name --flavor default
```

Creates a slidev project in `~/sldr/presentations/presentation-name/`.

## JSON Output Format

All `--json` commands return:

```json
{
  "success": true,
  "dry_run": false,
  "data": { ... }
}
```

On error:
```json
{
  "success": false,
  "error": "Error message",
  "cause": "Detailed cause"
}
```

## Useful Commands

```bash
# List available resources
sldr ls slides          # List all slides
sldr ls skeletons       # List all skeletons  
sldr ls templates       # List available templates
sldr ls flavors         # List available flavors

# Create missing slides from skeleton
sldr slides derive skeleton-name --template default

# Open presentation in browser
sldr open presentation-name
```

## Slide Content Tips

- Start content with `# Title` for headings
- Use `::left::` and `::right::` for two-column layouts
- Code blocks: Use standard markdown fenced code blocks
- Images: Reference from `/public/` directory in slidev project

## Example: Complete Workflow

```bash
# 1. Create slides JSON
cat > /tmp/slides.json << 'EOF'
{
  "directory": "rust-intro",
  "slides": [
    {
      "name": "title",
      "title": "Introduction to Rust",
      "layout": "cover",
      "tags": ["rust", "intro"],
      "content": "# Introduction to Rust\n\nA systems programming language"
    },
    {
      "name": "why-rust",
      "title": "Why Rust?",
      "layout": "default",
      "tags": ["rust", "benefits"],
      "content": "# Why Rust?\n\n- Memory safety without GC\n- Zero-cost abstractions\n- Fearless concurrency"
    }
  ]
}
EOF

# 2. Create slides
sldr slides create --file /tmp/slides.json --json

# 3. Create skeleton JSON
cat > /tmp/skeleton.json << 'EOF'
{
  "name": "rust-talk",
  "title": "Introduction to Rust",
  "slides": ["rust-intro/title", "rust-intro/why-rust"],
  "flavor": "default"
}
EOF

# 4. Create skeleton
sldr skeleton create --file /tmp/skeleton.json --json

# 5. Validate
sldr skeleton validate rust-talk --json

# 6. Build
sldr build rust-talk
```
