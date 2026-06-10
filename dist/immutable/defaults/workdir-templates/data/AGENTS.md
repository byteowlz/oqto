# Data Analysis

This workspace is for exploring, analyzing, and visualizing data.

## Tools

### Python (via uv)

Primary analysis stack. Use uv for all Python execution:

```bash
uv init                    # Initialize project (first time)
uv add pandas matplotlib seaborn plotly sqlite-utils  # Common deps
uv run script.py           # Run analysis script
uv run python -c "..."     # Quick one-liners
```

Common libraries to use:
- **pandas**: DataFrames, CSV/Excel/JSON loading, transformations
- **matplotlib/seaborn**: Static charts and plots
- **plotly**: Interactive charts (outputs HTML)
- **sqlite-utils**: Quick SQLite operations from CLI or Python
- **polars**: Fast alternative to pandas for large datasets

### SQLite

For structured data analysis:

```bash
sqlite3 data.db ".schema"              # Inspect schema
sqlite3 data.db "SELECT ..."           # Query
sqlite3 -csv data.db "SELECT ..." > out.csv  # Export
```

### File Formats

Read and write: CSV, TSV, JSON, JSONL, Parquet, Excel (.xlsx), SQLite.

## File Organization

```
~/oqto/data/
  datasets/          # Raw input data (never modify originals)
  analysis/          # Analysis scripts and notebooks
  output/            # Generated charts, reports, exports
  README.md          # Index of datasets and analyses
```

## Workflow

1. User provides data (file path, paste, or URL)
2. Load and inspect: shape, columns, types, nulls, basic stats
3. Clean: handle missing values, fix types, remove duplicates
4. Analyze: aggregations, correlations, trends, outliers
5. Visualize: charts saved as PNG/SVG/HTML to `output/`
6. Summarize findings in plain language

## Guidelines

- Always inspect data before analysis (head, shape, dtypes, describe)
- Keep raw data untouched -- write cleaned versions to separate files
- Save charts with descriptive filenames: `output/sales-by-region-2025.png`
- Use matplotlib for simple static charts, plotly for interactive ones
- For large files (>100MB), use polars or sqlite instead of pandas
- Show your work: print intermediate results so the user can follow
- When the user asks "what's interesting", look for: outliers, trends, correlations, missing patterns
- Always label axes, add titles, use readable fonts on charts
