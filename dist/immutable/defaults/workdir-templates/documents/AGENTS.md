# Documents & Presentations

This workspace is for creating professional documents and presentations using sldr and tmpltr.

## Tools

### sldr -- Slide Presentations

Build markdown-based presentations powered by Slidev.

```bash
sldr ls slides              # List available slide templates
sldr ls presentations       # List existing presentations
sldr ls flavors             # List visual themes
sldr new "slide title"      # Create a new slide
sldr build <skeleton.md>    # Build presentation from skeleton
sldr add <skeleton.md> <slide.md>   # Add slide to presentation
sldr preview <slide.md>     # Quick preview a single slide
sldr open <presentation>    # Open in Slidev (browser)
sldr search "query"         # Search slides by content/tags
```

**Workflow:**
1. Create a skeleton file (outline of slide order)
2. Create individual slides with `sldr new` or write them manually
3. Build with `sldr build skeleton.md`
4. Preview and iterate

Slides live in `~/.local/share/sldr/slides/`. Presentations output to the current directory.

### tmpltr -- Document Generation

Generate professional PDFs from structured TOML data + Typst templates.

```bash
tmpltr templates            # List available templates
tmpltr brands               # List available brands (logos, colors, fonts)
tmpltr new <template>       # Create content file from template
tmpltr compile <file.toml>  # Compile to PDF
tmpltr compile <file.toml> --brand <name>  # Compile with brand
tmpltr watch <file.toml>    # Watch and recompile on changes
tmpltr blocks <file.toml>   # List editable content blocks
tmpltr set <file.toml> <path> <value>  # Set a block value
tmpltr example              # Generate example template + content pair
```

**Workflow:**
1. Pick a template: `tmpltr templates`
2. Create content: `tmpltr new invoice` (creates invoice.toml)
3. Fill in content blocks (edit the TOML)
4. Compile: `tmpltr compile invoice.toml --brand byteowlz -o invoice.pdf`

## File Organization

```
~/oqto/documents/
  presentations/     # sldr presentations
  documents/         # tmpltr documents
  templates/         # Custom templates
  output/            # Generated PDFs
```

Create subdirectories per project or client as needed.

## Guidelines

- Always preview before finalizing
- Use brands for consistent corporate identity
- Keep slide content concise -- one idea per slide
- For documents, fill all required blocks before compiling
- Save generated PDFs to `output/` with descriptive names
