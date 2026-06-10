# sldr JSON Schemas

## Slide Input Schema

Used with `sldr slides create --file`:

```json
{
  "directory": "optional-subdirectory",
  "slides": [
    {
      "name": "slide-filename",
      "title": "Slide Title",
      "content": "# Markdown content",
      "description": "Optional description",
      "layout": "default",
      "tags": ["tag1", "tag2"],
      "directory": "optional-override-dir"
    }
  ]
}
```

**Required fields per slide**: `name`, `title`, `content`

**Optional fields**: `description`, `layout` (default: "default"), `tags` (default: []), `directory`

## Skeleton Input Schema

Used with `sldr skeleton create --file`:

```json
{
  "name": "skeleton-filename",
  "title": "Presentation Title",
  "slides": ["dir/slide1", "dir/slide2"],
  "description": "Optional description",
  "flavor": "default",
  "slidev_config": {
    "theme": "default",
    "transition": "slide-left",
    "aspect_ratio": "16/9",
    "canvas_width": 1280,
    "drawings": true,
    "dark_mode": false,
    "record": false
  }
}
```

**Required fields**: `name`, `title`, `slides`

**Optional fields**: `description`, `flavor`, `slidev_config`

## Response Format

All `--json` commands return:

**Success**:
```json
{
  "success": true,
  "dry_run": false,
  "data": {
    "created_count": 3,
    "failed_count": 0,
    "total": 3,
    "created": [
      {"name": "slide1", "path": "/full/path/slide1.md"}
    ],
    "failed": []
  }
}
```

**Dry Run**:
```json
{
  "success": true,
  "dry_run": true,
  "data": { ... }
}
```

**Error**:
```json
{
  "success": false,
  "error": "Error message",
  "cause": "Detailed cause if available"
}
```
