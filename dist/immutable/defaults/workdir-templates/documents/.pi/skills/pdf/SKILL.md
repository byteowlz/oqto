---
name: pdf
description: Complete toolkit for handling PDF files—extracting text and tables, generating documents, merging/splitting, and populating forms. Deploy when you need to process, create, or examine PDFs programmatically.
license: MIT
---

# PDF Operations

## What's Inside

This resource outlines fundamental PDF operations using Python libraries and command-line utilities. For advanced capabilities, JavaScript alternatives, and extended examples, see reference.md. For form completion details, consult forms.md.

## Quick Reference

```python
from pypdf import PdfReader, PdfWriter

# Load document
reader = PdfReader("file.pdf")
print(f"Pages: {len(reader.pages)}")

# Extract text
text = ""
for page in reader.pages:
    text += page.extract_text()
```

## Python Libraries

### pypdf — Core Operations

**Merge documents:**
```python
from pypdf import PdfWriter, PdfReader

writer = PdfWriter()
for pdf in ["a.pdf", "b.pdf", "c.pdf"]:
    reader = PdfReader(pdf)
    for page in reader.pages:
        writer.add_page(page)

with open("combined.pdf", "wb") as f:
    writer.write(f)
```

**Split document:**
```python
reader = PdfReader("source.pdf")
for i, page in enumerate(reader.pages):
    writer = PdfWriter()
    writer.add_page(page)
    with open(f"page_{i+1}.pdf", "wb") as f:
        writer.write(f)
```

**Metadata:**
```python
reader = PdfReader("file.pdf")
meta = reader.metadata
print(f"Title: {meta.title}")
print(f"Author: {meta.author}")
```

**Rotate pages:**
```python
reader = PdfReader("input.pdf")
writer = PdfWriter()

page = reader.pages[0]
page.rotate(90)  # Clockwise
writer.add_page(page)

with open("rotated.pdf", "wb") as f:
    writer.write(f)
```

### pdfplumber — Text & Tables

**Text with layout:**
```python
import pdfplumber

with pdfplumber.open("file.pdf") as pdf:
    for page in pdf.pages:
        print(page.extract_text())
```

**Table extraction:**
```python
with pdfplumber.open("file.pdf") as pdf:
    for i, page in enumerate(pdf.pages):
        tables = page.extract_tables()
        for j, table in enumerate(tables):
            print(f"Table {j+1} on page {i+1}:")
            for row in table:
                print(row)
```

**Tables to Excel:**
```python
import pandas as pd

with pdfplumber.open("file.pdf") as pdf:
    all_tables = []
    for page in pdf.pages:
        for table in page.extract_tables():
            if table:
                df = pd.DataFrame(table[1:], columns=table[0])
                all_tables.append(df)
    
    if all_tables:
        pd.concat(all_tables).to_excel("output.xlsx", index=False)
```

### reportlab — PDF Creation

**Simple PDF:**
```python
from reportlab.lib.pagesizes import letter
from reportlab.pdfgen import canvas

c = canvas.Canvas("output.pdf", pagesize=letter)
width, height = letter

c.drawString(100, height - 100, "Hello World!")
c.line(100, height - 140, 400, height - 140)
c.save()
```

**Multi-page:**
```python
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, PageBreak
from reportlab.lib.styles import getSampleStyleSheet

doc = SimpleDocTemplate("report.pdf", pagesize=letter)
styles = getSampleStyleSheet()
story = []

story.append(Paragraph("Title", styles['Title']))
story.append(Spacer(1, 12))
story.append(Paragraph("Content here..." * 20, styles['Normal']))
story.append(PageBreak())
story.append(Paragraph("Page 2", styles['Heading1']))

doc.build(story)
```

## Command-Line Tools

### pdftotext
```bash
# Extract text
pdftotext input.pdf output.txt

# Preserve layout
pdftotext -layout input.pdf output.txt

# Pages 1-5 only
pdftotext -f 1 -l 5 input.pdf output.txt
```

### qpdf
```bash
# Merge
qpdf --empty --pages a.pdf b.pdf -- output.pdf

# Split ranges
qpdf input.pdf --pages . 1-5 -- part1.pdf
qpdf input.pdf --pages . 6-10 -- part2.pdf

# Rotate
qpdf input.pdf output.pdf --rotate=+90:1

# Decrypt
qpdf --password=pass --decrypt encrypted.pdf decrypted.pdf
```

### pdftk
```bash
# Merge
pdftk a.pdf b.pdf cat output combined.pdf

# Split all pages
pdftk input.pdf burst

# Rotate
pdftk input.pdf rotate 1east output rotated.pdf
```

## Common Tasks

### OCR for Scanned PDFs
```python
import pytesseract
from pdf2image import convert_from_path

images = convert_from_path('scanned.pdf')
text = ""
for i, image in enumerate(images):
    text += f"--- Page {i+1} ---\n"
    text += pytesseract.image_to_string(image)
print(text)
```

### Add Watermark
```python
from pypdf import PdfReader, PdfWriter

watermark = PdfReader("watermark.pdf").pages[0]
reader = PdfReader("document.pdf")
writer = PdfWriter()

for page in reader.pages:
    page.merge_page(watermark)
    writer.add_page(page)

with open("watermarked.pdf", "wb") as f:
    writer.write(f)
```

### Extract Images
```bash
pdfimages -j input.pdf prefix
# Creates prefix-000.jpg, prefix-001.jpg, etc.
```

### Password Protection
```python
from pypdf import PdfReader, PdfWriter

reader = PdfReader("input.pdf")
writer = PdfWriter()

for page in reader.pages:
    writer.add_page(page)

writer.encrypt("userpass", "ownerpass")
with open("secure.pdf", "wb") as f:
    writer.write(f)
```

## Tool Selection

| Goal | Tool | Method |
|------|------|--------|
| Merge | pypdf | `writer.add_page(page)` |
| Split | pypdf | One per file |
| Text extraction | pdfplumber | `page.extract_text()` |
| Table extraction | pdfplumber | `page.extract_tables()` |
| Create PDF | reportlab | Canvas or Platypus |
| CLI merge | qpdf | `qpdf --empty --pages ...` |
| OCR | pytesseract | Convert to images first |
| Forms | pdf-lib or pypdf | See forms.md |

## See Also

- reference.md — Advanced pypdfium2, JavaScript pdf-lib
- forms.md — Form filling instructions
