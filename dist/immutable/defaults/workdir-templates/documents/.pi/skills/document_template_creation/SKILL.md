---
name: document-template-creation
description: Create professional document templates for tmpltr based on existing files (docx, pdf) or from scratch. Supports any document type - invoices, letters, proposals, contracts, reports, certificates, and more. Features JSON pipeline for agent-driven document generation. Analyze input files to extract structure and generate matching typst templates with JSON schemas for validated data input. Use when the user wants to create new document templates, convert existing documents to tmpltr format, generate template variations, or pipe JSON data into PDF output.
---

# Document Template Creation

Create professional, customizable document templates for tmpltr that separate content from presentation. Every template ships with a JSON schema for validated data input and supports a direct JSON-to-PDF pipeline.

## JSON Pipeline (Agent Workflow)

The fastest path from data to document. No intermediate files needed.

### One-shot: JSON to PDF

```bash
# Pipe JSON directly to PDF
echo '{"seller":{"company":"Acme"},"buyer":{"name":"Globex"},"items":[{"description":"Consulting","qty":10,"price":150}]}' \
  | tmpltr pipe invoice -o invoice.pdf

# From a JSON file
tmpltr pipe invoice -d order.json -o invoice.pdf

# With brand
tmpltr pipe invoice -d order.json --brand mycompany -o invoice.pdf
```

### Fill: JSON to TOML (for editing)

```bash
# Create editable TOML from JSON data
tmpltr fill invoice -d customer.json -o invoice.toml

# With empty data (get all defaults)
tmpltr fill invoice -o invoice.toml

# Then edit and compile
vim invoice.toml
tmpltr compile invoice.toml -o invoice.pdf
```

### Compile with JSON Overrides

```bash
# Override specific fields at compile time
tmpltr compile invoice.toml --data '{"buyer":{"name":"Special Client"}}' -o special.pdf
```

### Schema: Know What Fields to Provide

```bash
# Print schema to stdout
tmpltr schema invoice

# Save to file
tmpltr schema invoice -o invoice.schema.json

# Use schema for validation in your pipeline
```

### Example: Full Agent Pipeline

```bash
# 1. Get the schema to know what data is needed
tmpltr schema invoice -o invoice.schema.json

# 2. Build JSON data (from API, database, user input, etc.)
cat > order.json << 'EOF'
{
  "seller": {
    "company": "Acme Corp",
    "address": "Innovation Drive 1",
    "city": "Berlin",
    "postal": "10115"
  },
  "buyer": {
    "name": "TechCorp GmbH",
    "address": "Innovation Street 42",
    "city": "Berlin"
  },
  "items": [
    {"description": "AI Consulting", "qty": 40, "price": 180},
    {"description": "Model Training", "qty": 20, "price": 250}
  ],
  "invoice": {
    "number": "INV-2026-0042",
    "date": "2026-02-12",
    "currency": "EUR",
    "tax_rate": 19
  },
  "payment": {
    "bank": "Commerzbank",
    "iban": "DE89 3704 0044 0532 0130 00",
    "bic": "COBADEFFXXX"
  }
}
EOF

# 3. Generate PDF in one shot
tmpltr pipe invoice -d order.json --brand mycompany -o invoice.pdf
```

## Quick Start

### Option 1: Modify an Existing Template

```bash
# Find a similar template
tmpltr templates

# Generate starter content (with all defaults)
tmpltr new invoice -o my-invoice.toml

# Edit and test
vim my-invoice.toml
tmpltr compile my-invoice.toml -o test.pdf
```

### Option 2: From Existing Document

```bash
# Copy template from skill assets
cp assets/templates/invoice.typ my-invoice.typ
cp assets/templates/invoice.toml my-invoice.toml

# Customize for your needs
vim my-invoice.typ
vim my-invoice.toml

# Test
tmpltr compile my-invoice.toml -o output.pdf
```

### Option 3: From Scratch

1. Review [document-types.md](references/document-types.md) for document structure
2. Copy a similar template as starting point
3. Adapt the typst template
4. Create corresponding content.toml
5. Generate JSON schema: `tmpltr schema my-template -o my-template.schema.json`
6. Test compilation

## Available Templates

Browse templates in `assets/templates/`:

| Template | Purpose | Schema | Complexity |
|----------|---------|--------|------------|
| `invoice.typ` | Payment requests | `invoice.schema.json` | Low |
| `formal-letter.typ` | Business correspondence | `formal-letter.schema.json` | Low |
| `report.typ` | Technical/business reports | `report.schema.json` | Medium |
| `certificate.typ` | Awards and credentials | `certificate.schema.json` | Low |
| `simple-quote.typ` | Service estimates | - | Low |
| `agenda.typ` | Meeting agendas | - | Low |
| `protokoll.typ` | Meeting protocols | - | Medium |
| `angebot.typ` | Project proposals (German) | - | Medium |

## Template Structure

### Minimal Template

```typst
#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get

#let data = tmpltr-data()

#set page(paper: "a4", margin: 2.5cm)
#set text(font: "Helvetica Neue", size: 11pt)

// Your content here
#get(data, "title", default: "Untitled")
#get(data, "content", default: "")
```

### Full Template with Features

```typst
#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-color, brand-logo-image

#let data = tmpltr-data()
#let has-brand = "brand" in data and data.brand != none

// Brand-aware colors
#let primary = if has-brand { 
  brand-color(data, "primary", default: "#1e293b") 
} else { 
  get(data, "colors.primary", default: "#1e293b") 
}

// Page setup
#set page(
  paper: "a4",
  margin: (top: 2cm, bottom: 2cm, left: 2.5cm, right: 2.5cm),
)

// Typography  
#set text(
  font: if has-brand { 
    get(data, "brand.fonts.body", default: "Helvetica Neue") 
  } else { "Helvetica Neue" },
  size: 11pt,
)

// Logo if brand available
#if has-brand {
  place(top + right, brand-logo-image(data, width: 4cm))
}

// Document content
#get(data, "document.title", default: "Title")
```

## Data Access Patterns

### Simple Field Access

```typst
#get(data, "invoice.number", default: "INV-0001")
```

### Nested Objects

```typst
#get(data, "seller.company", default: "Your Company")
#get(data, "seller.address", default: "123 Street")
```

### Arrays (Items, Rows)

JSON input:
```json
{
  "items": [
    {"description": "Service A", "qty": 1, "price": 100},
    {"description": "Service B", "qty": 2, "price": 200}
  ]
}
```

TOML equivalent:
```toml
[[items]]
description = "Service A"
qty = 1
price = 100

[[items]]
description = "Service B" 
qty = 2
price = 200
```

Template:
```typst
#let items = get(data, "items", default: ())
#for item in items {
  item.description 
  str(item.qty)
  str(item.price)
}
```

### Conditional Content

```typst
#if get(data, "show_logo", default: true) [
  // Show logo
]

#let notes = get(data, "notes", default: "")
#if notes != "" [
  #heading()[Notes]
  #notes
]
```

## Content TOML Structure

### Basic Structure

```toml
[meta]
template = "template.typ"
template_id = "template-name"
template_version = "1.0.0"

[document]
title = "Document Title"
date = "2025-01-15"

[document.sender]
name = "Sender Name"
email = "sender@example.com"

[document.recipient]
name = "Recipient Name"

[[items]]
description = "Item 1"
amount = 100

[labels]
title = "Title"
date = "Date"
```

### Blocks for Rich Content

```toml
[blocks.introduction]
title = "Introduction"
format = "markdown"
content = """
This is **bold** and *italic* text with:
- Bullet points
- Numbered lists

## Subheadings work too
"""

[blocks.timeline]
title = "Project Timeline"
type = "table"
columns = ["Phase", "Duration", "Deliverable"]
rows = [
  ["Phase 1", "4 weeks", "Requirements"],
  ["Phase 2", "8 weeks", "Development"],
]
```

## JSON Schema for Templates

Every template should have a corresponding JSON schema. Generate with:

```bash
tmpltr schema <template> -o <template>.schema.json
```

The schema describes all fields the template reads, their types, and default values. This enables:

- **Validation** of JSON/TOML input before compilation
- **IDE autocompletion** when editing data files
- **Agent tooling** that knows exactly what data to provide
- **API integration** with schema-based request validation

### Schema Structure

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://tmpltr.dev/schemas/invoice.schema.json",
  "title": "invoice content",
  "description": "JSON/TOML schema for invoice template",
  "type": "object",
  "properties": {
    "seller": {
      "type": "object",
      "properties": {
        "company": { "type": "string", "default": "Your Company" },
        "address": { "type": "string", "default": "123 Business Street" }
      }
    },
    "buyer": { ... },
    "items": { ... }
  }
}
```

## Brand Integration

Templates should support `--brand` flag:

```typst
#let has-brand = "brand" in data and data.brand != none

// Logo
#if has-brand {
  brand-logo-image(data, width: 4cm)
}

// Colors
#let primary = brand-color(data, "primary", default: "#000000")

// Fonts
#set text(font: brand-font(data, usage: "body", default: "Arial"))

// Contact info
#get(data, "brand.contact.company", default: "Company")
```

## Localization

Add labels section for multi-language support:

```toml
[labels]
invoice = "Rechnung"
number = "Rechnungsnummer"
date = "Datum"
total = "Gesamtsumme"
```

```typst
#get(data, "labels.invoice", default: "Invoice")
```

## Best Practices

### Template Design

1. **Start simple**: Get basic structure working first
2. **Use defaults**: Every field should have a fallback
3. **Support branding**: Check for brand data and use it
4. **Document structure**: Add comments with field IDs
5. **Test edge cases**: Empty content, long text, special chars
6. **Generate schema**: Run `tmpltr schema` after any template change

### Content Organization

1. **Group related fields**: `[invoice.number]`, `[invoice.date]`
2. **Use arrays for lists**: `[[items]]` for line items
3. **Add labels for i18n**: All UI text in `[labels]`
4. **Include examples**: Realistic placeholder content
5. **Comment liberally**: Explain field purposes

### Testing Checklist

- [ ] Compiles without errors
- [ ] All fields render correctly
- [ ] JSON pipe works: `echo '{}' | tmpltr pipe <template> -o test.pdf`
- [ ] Tables format properly
- [ ] Multiline content displays correctly
- [ ] Brand integration works (`--brand` flag)
- [ ] Empty values don't break layout
- [ ] Long text wraps appropriately
- [ ] Special characters escaped properly
- [ ] Schema generated and accurate

## Common Patterns

### Tables with Line Items

```typst
#let items = get(data, "items", default: ())
#table(
  columns: (3fr, 1fr, 1fr),
  [*Description*], [*Qty*], [*Price*],
  ..items.map(item => (
    item.description,
    str(item.quantity),
    item.price
  )).flatten()
)
```

### Signature Blocks

```typst
#grid(
  columns: (1fr, 1fr),
  column-gutter: 2em,
  [
    #line(length: 80%)
    #get(data, "signer.name")
    #get(data, "signer.title")
  ],
  [
    #line(length: 80%)
    #get(data, "recipient.name")
  ]
)
```

### Conditional Sections

```typst
#let terms = get(data, "terms", default: "")
#if terms != "" [
  #heading()[Terms & Conditions]
  #terms
]
```

### Date Formatting

```typst
// Simple string
#get(data, "date", default: "2025-01-15")

// Using datetime (if using typst datetime)
#let dt = datetime.today()
#dt.display("[day].[month].[year]")
```

## Troubleshooting

### Template Not Found

```bash
# Check template paths
tmpltr templates

# Use absolute path
tmpltr init /full/path/to/template.typ
```

### Missing Fields

If `tmpltr new` generates incomplete content:
1. Run with `--analyze-data` flag: `tmpltr init template.typ --analyze-data`
2. Manually add missing fields to content.toml
3. Or use `-c existing.toml` to base on existing content

### Compilation Errors

```bash
# Check template syntax
typst compile template.typ --input-data='{}'

# Validate content
tmpltr validate content.toml
```

### JSON Pipe Errors

```bash
# Check what schema expects
tmpltr schema invoice

# Test with minimal JSON first
echo '{}' | tmpltr pipe invoice -o test.pdf

# Add fields incrementally
echo '{"items":[{"description":"Test","qty":1,"price":100}]}' | tmpltr pipe invoice -o test.pdf
```

## Advanced Features

### JSON Schema Generation

```bash
# Generate schema for a template
tmpltr schema invoice -o invoice.schema.json

# Generate schema alongside content
tmpltr fill invoice --schema -o invoice.toml
```

### Watch Mode

```bash
# Auto-recompile on changes
tmpltr watch content.toml -o output.pdf --open
```

### Batch Document Generation

```bash
# Generate multiple documents from JSON array
for order in $(cat orders.json | jq -c '.[]'); do
  id=$(echo "$order" | jq -r '.invoice.number')
  echo "$order" | tmpltr pipe invoice -o "invoices/${id}.pdf"
done
```

## Examples

See `assets/templates/` for complete, working examples with schemas:

- **invoice.typ/toml/schema.json**: Clean commercial invoice with tax
- **formal-letter.typ/toml/schema.json**: DIN 5008 compliant business letter
- **report.typ/toml/schema.json**: Multi-section technical report
- **certificate.typ/toml/schema.json**: Landscape award certificate

Copy and modify these as starting points for your own templates.
