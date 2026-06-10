---
name: ms-excel-xlsx
description: Toolkit for spreadsheet work—building files with formulas and formatting, analyzing data, modifying existing workbooks while preserving formulas, creating visualizations, and recalculating. Use for .xlsx, .xlsm, .csv, .tsv files and similar formats.
license: MIT
---

# Excel & Spreadsheet Operations

## Quality Standards

### All Spreadsheets

**Zero Errors Required:**
Every workbook must ship with no formula errors—no #REF!, #DIV/0!, #VALUE!, #N/A, #NAME?

**Respect Existing Patterns:**
When updating templates, match existing formatting exactly. Don't override established conventions.

### Financial Models

**Color Coding:**

- **Blue (RGB 0,0,255):** Hardcoded inputs, user-changeable values
- **Black (RGB 0,0,0):** All formulas and calculations
- **Green (RGB 0,128,0):** Links to other sheets in same workbook
- **Red (RGB 255,0,0):** External file links
- **Yellow background (RGB 255,255,0):** Key assumptions or cells needing updates

**Number Formats:**

- **Years:** As text ("2024" not "2,024")
- **Currency:** $#,##0 with units in headers ("Revenue ($mm)")
- **Zeros:** Display as "-" using format like `$#,##0;($#,##0);-`
- **Percentages:** 0.0% (one decimal)
- **Multiples:** 0.0x format
- **Negatives:** Parentheses (123) not minus signs

**Formula Guidelines:**

- All assumptions go in dedicated cells—never hardcode in formulas
- Use cell references: `=B5*(1+$B$6)` not `=B5*1.05`
- Verify references, check for off-by-one errors
- Test edge cases: zeros, negatives
- Watch for circular references

**Document Sources:**
Format: "Source: [System/Document], [Date], [Reference], [URL]"
Examples:

- "Source: Company 10-K, FY2024, Page 45, Revenue Note"
- "Source: Bloomberg Terminal, 8/15/2025, AAPL US Equity"
- "Source: FactSet, 8/20/2025, Consensus Estimates"

## Reading & Analysis

### With pandas

```python
import pandas as pd

# Load workbook
df = pd.read_excel('file.xlsx')  # First sheet
all_sheets = pd.read_excel('file.xlsx', sheet_name=None)  # All sheets

# Explore
df.head()      # Preview
df.info()      # Column info
df.describe()  # Statistics

# Export
df.to_excel('output.xlsx', index=False)
```

## Workflows

### CRITICAL: Use Excel Formulas, Not Python Calculations

**WRONG—Hardcoding Python results:**

```python
total = df['Sales'].sum()
sheet['B10'] = total  # Hardcodes 5000

growth = (df.iloc[-1]['Rev'] - df.iloc[0]['Rev']) / df.iloc[0]['Rev']
sheet['C5'] = growth  # Hardcodes 0.15
```

**RIGHT—Excel formulas:**

```python
sheet['B10'] = '=SUM(B2:B9)'
sheet['C5'] = '=(C4-C2)/C2'
sheet['D20'] = '=AVERAGE(D2:D19)'
```

This applies to totals, percentages, ratios—all calculations. The spreadsheet should recalculate when data changes.

### Standard Process

1. **Choose tool:** pandas for data, openpyxl for formulas/formatting
2. **Load or create:** Open existing or new workbook
3. **Modify:** Edit cells, add formulas, apply formatting
4. **Save:** Write to file
5. **Recalculate (MANDATORY with formulas):**

   ```bash
   python formula_recalc.py output.xlsx
   ```

6. **Fix errors:** Check JSON output for #REF!, #DIV/0!, etc.

### Creating New Files

```python
from openpyxl import Workbook
from openpyxl.styles import Font, PatternFill, Alignment

wb = Workbook()
sheet = wb.active

# Data
sheet['A1'] = 'Header'
sheet.append(['Row', 'data'])

# Formula
sheet['B2'] = '=SUM(A1:A10)'

# Formatting
sheet['A1'].font = Font(bold=True, color='FF0000')
sheet['A1'].fill = PatternFill('solid', start_color='FFFF00')
sheet.column_dimensions['A'].width = 20

wb.save('output.xlsx')
```

### Editing Existing Files

```python
from openpyxl import load_workbook

wb = load_workbook('existing.xlsx')
sheet = wb.active  # Or wb['SheetName']

# Multi-sheet
for name in wb.sheetnames:
    sheet = wb[name]

# Changes
sheet['A1'] = 'New'
sheet.insert_rows(2)
sheet.delete_cols(3)

# New sheet
new_sheet = wb.create_sheet('New')

wb.save('modified.xlsx')
```

## Formula Recalculation

Files modified by openpyxl contain formula strings without calculated values.

**Recalculate:**

```bash
python formula_recalc.py file.xlsx [timeout]
```

Example:

```bash
python formula_recalc.py model.xlsx 30
```

The script:

- Configures LibreOffice macro on first run
- Recalculates all formulas
- Scans for errors (#REF!, #DIV/0!, etc.)
- Returns JSON with error locations

## Verification Checklist

**Essential:**

- [ ] Test 2-3 sample references before building full model
- [ ] Confirm column mapping (column 64 = BL, not BK)
- [ ] Remember: Excel rows are 1-indexed (DataFrame row 5 = Excel row 6)

**Common Mistakes:**

- [ ] Check for NaN with `pd.notna()`
- [ ] Watch far-right columns (FY data often 50+)
- [ ] Search all occurrences, not just first
- [ ] Guard against division by zero
- [ ] Verify all cell references
- [ ] Use correct cross-sheet format: `Sheet1!A1`

**Testing:**

- [ ] Test formulas on 2-3 cells first
- [ ] Verify referenced cells exist
- [ ] Test with zeros, negatives, large values

**JSON Output Format:**

```json
{
  "status": "success",
  "total_errors": 0,
  "total_formulas": 42,
  "error_summary": {
    "#REF!": {
      "count": 2,
      "locations": ["Sheet1!B5", "Sheet1!C10"]
    }
  }
}
```

## Best Practices

**Tool Selection:**

- **pandas:** Data analysis, bulk operations, simple export
- **openpyxl:** Complex formatting, formulas, Excel-specific features

**openpyxl Tips:**

- Cells are 1-indexed (row=1, col=1 is A1)
- `data_only=True` reads calculated values (WARNING: saving loses formulas)
- Large files: `read_only=True` or `write_only=True`
- Formulas need recalculation with formula_recalc.py

**pandas Tips:**

- Specify dtypes: `pd.read_excel('file.xlsx', dtype={'id': str})`
- Read specific columns: `usecols=['A', 'C', 'E']`
- Parse dates: `parse_dates=['date_col']`

**Code Style:**

- Keep Python code concise
- No verbose variable names
- Minimal print statements
- Add cell comments for complex formulas
- Document hardcoded value sources
