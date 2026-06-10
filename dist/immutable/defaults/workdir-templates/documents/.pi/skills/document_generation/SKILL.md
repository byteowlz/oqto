---
name: document_generation
description: This skill should be used to create high quality pdf documents based on predefined templates. When the user asks for a document, you can list all available templates and brands and ask the user for the content to put into the template. You only edit the toml templates, not the .typ files. The toml/typ pair compile into a professional looking pdf via tmpltr.
license: MIT
---

# document generation with tmpltr

You can use the included templates that tmpltr ships with or ask the user for a toml/typ pair which they can upload.

tmpltr compiles TOML content files into PDFs using Typst templates, with support for brand management (logos, fonts, colors).

## Core Concept

```
[Content File .toml] + [Template .typ] + [Brand (optional)] --> tmpltr compile --> [Output .pdf]
```

The content file contains structured data and text blocks. The template defines layout and styling. Brands provide reusable visual identity assets. tmpltr merges them via Typst.

## Quick Start

```bash
# Compile content to PDF
tmpltr compile content.toml -o output.pdf

# Compile with a specific brand
tmpltr compile content.toml --brand byteowlz -o output.pdf
```

## Content File Structure (.toml)

```toml
[meta]
template = "my-template.typ"  # Path relative to content file location
template_id = "my-template"
template_version = "1.0.0"

# Brand settings (optional, can be overridden with --brand flag)
[brand]
logo = "path/to/logo.svg"

[brand.colors]
primary = "#0f172a"
accent = "#38bdf8"

# Structured data - access as data.quote.* in template
[quote]
number = "Q-2025-001"
title = "Project Title"
date = "2025-12-09"

[quote.client]
name = "Client Name"
address = "123 Street"

# Arrays use [[double.brackets]]
[[quote.line_items]]
description = "Item 1"
amount = "$100"

[[quote.line_items]]
description = "Item 2"
amount = "$200"

# Content blocks - access as data.blocks.* in template
[blocks.description]
title = "Project Description"
format = "markdown"
content = """
Multi-line content with **markdown** supported.
- Bullet points work
- Like this
"""

[blocks.timeline]
title = "Timeline"
type = "table"
columns = ["Phase", "Duration"]
rows = [
    ["Phase 1", "2 weeks"],
    ["Phase 2", "4 weeks"],
]
```

## Template Structure (.typ)

**IMPORTANT: Use a single .typ file per template.** Do not split templates into multiple files (e.g., main.typ + template.typ). Keep everything in one self-contained file for simplicity.

### Complete Template Example

```typst
// =============================================================================
// My Template (tmpltr edition)
//
// Usage: tmpltr compile content.toml -o output.pdf
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, editable, get, brand-color, brand-logo, brand-font

// =============================================================================
// HELPER FUNCTIONS (define these before using them)
// =============================================================================

#let my-helper(data) = {
  // Helper logic here
}

// =============================================================================
// TEMPLATE FUNCTION
// =============================================================================

#let my-template(data, doc) = {
  // Brand configuration with safe defaults
  let colors = (
    primary: brand-color(data, "primary", default: "#000000"),
    accent: brand-color(data, "accent", default: "#0066cc"),
    text: brand-color(data, "text", default: "#000000"),
  )

  let primary-font = brand-font(data, usage: "body", default: "Arial")

  // Page setup
  set page(paper: "a4", margin: 2.5cm)
  set text(font: primary-font, size: 10pt, fill: rgb(colors.text))

  // Document content here...

  doc
}

// =============================================================================
// APPLY TEMPLATE (always at the end)
// =============================================================================

#let data = tmpltr-data()
#show: doc => my-template(data, doc)
```

## Typst Tips and Gotchas

### List Markers (Bullet Points)

The default list markers can be too large. Here are options:

```typst
// Option 1: Small symbol with controlled size (RECOMMENDED)
set list(
  marker: text(size: 6pt, baseline: -2pt)[#sym.square.filled],
  indent: 0pt,
  body-indent: 8pt,
)

// Option 2: Unicode character (scales with text - may be too large)
set list(marker: [▪])  // U+25AA Black Small Square

// Option 3: Fixed-size box (requires manual baseline adjustment)
set list(marker: box(width: 4pt, height: 4pt, fill: black, baseline: 2pt))
```

**Note:** Unicode characters and symbols scale with the surrounding text size. To control bullet size independently, wrap in `text(size: Xpt)[]` or use a `box()`.

### Safe Data Access

Always use defaults when accessing data to avoid errors:

```typst
// Using get() helper (recommended)
#get(data, "workshop.title", default: "Untitled")

// Using .at() method
#data.at("workshop", default: (:)).at("title", default: "Untitled")

// For arrays
#for item in data.at("items", default: ()) [
  #item.at("name", default: "")
]
```

### Color Handling

Always wrap color strings in `rgb()`:

```typst
// Correct
#text(fill: rgb(colors.primary))[Text]
#line(stroke: 0.5pt + rgb(colors.text))

// Wrong - will error
#text(fill: colors.primary)[Text]
```

### Logo with Fallback

```typst
#let render-logo(data, width) = {
  let logo-path = brand-logo(data, variant: "primary", default: none)

  if logo-path != none {
    image(logo-path, width: width)
  } else {
    // Placeholder when no logo
    rect(width: width, height: 1.5cm, stroke: 0.5pt + luma(200))[
      #align(center + horizon)[#text(fill: luma(150), size: 9pt)[\[Logo\]]]
    ]
  }
}
```

### Parsing Lengths from TOML

TOML values come as strings, so parse them with `eval()`:

```typst
let logo-width-str = get(data, "brand.logo-width", default: "4cm")
let logo-width = if type(logo-width-str) == str {
  eval(logo-width-str)
} else {
  4cm
}
```

## Brand Management

Brands provide reusable visual identity assets (logos, fonts, colors, contact info).

### Brand Directory Structure

```
$XDG_DATA_HOME/tmpltr/brands/
  byteowlz/
    brand.toml       # Brand configuration
    logos/           # Logo files
      logo.svg
      logo-mono.svg
    fonts/           # Font files
      Inter-Regular.ttf
      Inter-Bold.ttf
```

### brand.toml Structure

```toml
id = "byteowlz"
default_language = "en"
languages = ["en", "de"]

[name]
en = "ByteOwlz"
de = "ByteOwlz GmbH"

[colors]
primary = "#0f172a"
secondary = "#64748b"
accent = "#38bdf8"
background = "#ffffff"
text = "#0b1120"

[logos]
primary = "logos/logo.svg"
monochrome = "logos/logo-mono.svg"

[typography.body]
family = "Inter"
files = ["fonts/Inter-Regular.ttf"]

[typography.heading]
family = "Inter"
files = ["fonts/Inter-Bold.ttf"]

[contact]
company = { en = "ByteOwlz GmbH" }
email = "hello@byteowlz.com"
website = "https://byteowlz.com"
```

### Brand CLI Commands

```bash
tmpltr brands list                    # List available brands
tmpltr brands show byteowlz           # Show brand details
tmpltr brands new mybrand             # Create new brand scaffold
tmpltr compile content.toml --brand byteowlz -o output.pdf
```

## CLI Commands Reference

```bash
# Compile content to PDF
tmpltr compile <content.toml> -o <output.pdf>
tmpltr compile <content.toml> --brand <brand-id> -o <output.pdf>

# Watch for changes and recompile
tmpltr watch <content.toml> -o <output.pdf>

# Content manipulation
tmpltr get <content.toml> blocks.intro       # Get block content
tmpltr set <content.toml> blocks.intro "New" # Set block content
tmpltr blocks <content.toml>                 # List all blocks
tmpltr validate <content.toml>               # Validate content file

# Brand management
tmpltr brands list
tmpltr brands show <brand-id>
tmpltr brands new <brand-id>

# Configuration
tmpltr config show
tmpltr config path
```

## tmpltr-lib Helper Functions

The `@local/tmpltr-lib:1.0.0` package provides these helpers:

| Function                                           | Description                         |
| -------------------------------------------------- | ----------------------------------- |
| `tmpltr-data()`                                    | Get parsed data from CLI input      |
| `get(data, path, default)`                         | Safely get nested value by dot-path |
| `editable(id, value, type, default)`               | Mark editable field                 |
| `editable-block(id, title, format)[body]`          | Mark editable block                 |
| `md(content)`                                      | Render markdown content             |
| `brand-color(data, name, default)`                 | Get brand color                     |
| `brand-logo(data, variant, default)`               | Get logo path                       |
| `brand-logo-image(data, variant, width, fallback)` | Render logo image                   |
| `brand-font(data, usage, default)`                 | Get font family                     |
| `brand-contact(data, field, default)`              | Get contact info                    |

## Template Data Access Patterns

| Content TOML         | Template Access                                           |
| -------------------- | --------------------------------------------------------- |
| `[meta]`             | `data.meta`                                               |
| `[quote]`            | `get(data, "quote")`                                      |
| `quote.number = "X"` | `get(data, "quote.number")`                               |
| `[quote.client]`     | `get(data, "quote.client")`                               |
| `[[quote.items]]`    | `data.at("quote", default: (:)).at("items", default: ())` |
| `[blocks.intro]`     | `get(data, "blocks.intro")`                               |
| `[brand.colors]`     | Use `brand-color()` helper                                |

## Error Troubleshooting

| Error                           | Cause                                             | Fix                                          |
| ------------------------------- | ------------------------------------------------- | -------------------------------------------- |
| "File name too long"            | Template uses `json()` instead of `json.decode()` | Use `tmpltr-data()`                          |
| "input file not found"          | Wrong template path                               | Template path is relative to content file    |
| "cannot access field"           | Missing data                                      | Use `get(data, "path", default: ...)`        |
| "unknown variable: tmpltr-data" | Missing import                                    | Add `#import "@local/tmpltr-lib:1.0.0": ...` |
| "brand not found"               | Brand doesn't exist                               | Run `tmpltr brands list`                     |

## Best Practices

1. **Single file templates**: Keep each template in ONE .typ file. Don't split unnecessarily.
2. **Use tmpltr-lib**: Import the helper library for cleaner templates
3. **Use get() for safety**: Always provide defaults for optional fields
4. **Brands for reusability**: Create brands for company visual identity
5. **Template paths**: Keep templates in same directory as content
6. **Control list markers**: Use `text(size: Xpt)[]` wrapper for predictable bullet sizes
7. **Always wrap colors**: Use `rgb(color-string)` when applying colors
8. **Parse TOML lengths**: Use `eval()` to convert string lengths like "4cm" to actual lengths
