# Context7 MCP Tools

*2 tools available*

**⚠️ Context7 has a 3-call limit per question.** Plan your calls carefully:
- Use `--library-id` when you know the ID (saves 1 call)
- The fetch-docs.sh skill uses max 2 calls, leaving 1 for your retry

## `resolve-library-id`

Resolves a package/product name to a Context7-compatible library ID and returns a list of matching libraries.

You MUST call this function before 'query-docs' to obtain a valid Context7-compatible library ID UNLESS the user explicitly provides a library ID in the format '/org/project' or '/org/project/version' in their query.

### Selection Process

1. Analyze the query to understand what library/package the user is looking for
2. Return the most relevant match based on:
   - Name similarity to the query (exact matches prioritized)
   - Description relevance to the query's intent
   - Documentation coverage (prioritize libraries with higher Code Snippet counts)
   - Source reputation (consider libraries with High or Medium reputation more authoritative)
   - Benchmark Score: Quality indicator (100 is the highest score)

### Parameters

- **`query`** (`string`) *(required)*: The user's original question or task. Used to rank library results by relevance.

- **`libraryName`** (`string`) *(required)*: Library name to search for and retrieve a Context7-compatible library ID.

### Response Format

Returns a list of matching libraries with:
- Title
- Context7-compatible library ID (e.g., `/reactjs/react.dev`)
- Code Snippets count
- Source Reputation (High/Medium/Low)
- Benchmark Score
- Description

### Examples

```bash
# Find React library
python scripts/mcp-client.py call -s "npx -y @upstash/context7-mcp" \
  -t resolve-library-id \
  -p '{"query": "useState hooks", "libraryName": "react"}'

# Find Next.js library
python scripts/mcp-client.py call -s "npx -y @upstash/context7-mcp" \
  -t resolve-library-id \
  -p '{"query": "routing", "libraryName": "next.js"}'
```

<details>
<summary>Full Schema</summary>

```json
{
  "type": "object",
  "properties": {
    "query": {
      "type": "string",
      "description": "The user's original question or task. Used to rank library results by relevance."
    },
    "libraryName": {
      "type": "string",
      "description": "Library name to search for and retrieve a Context7-compatible library ID."
    }
  },
  "required": ["query", "libraryName"]
}
```
</details>

## `query-docs`

Retrieves and queries up-to-date documentation and code examples from Context7 for any programming library or framework.

You must call 'resolve-library-id' first to obtain the exact Context7-compatible library ID required to use this tool, UNLESS the user explicitly provides a library ID in the format '/org/project' or '/org/project/version' in their query.

### Parameters

- **`libraryId`** (`string`) *(required)*: Exact Context7-compatible library ID (e.g., '/mongodb/docs', '/vercel/next.js') retrieved from 'resolve-library-id' or directly from user query.

- **`query`** (`string`) *(required)*: The question or task you need help with. Be specific and include relevant details. Good: 'How to set up authentication with JWT in Express.js'. Bad: 'auth'.

### Examples

```bash
# Get React hooks documentation
python scripts/mcp-client.py call -s "npx -y @upstash/context7-mcp" \
  -t query-docs \
  -p '{"libraryId": "/reactjs/react.dev", "query": "useState hooks examples"}'

# Get Next.js routing information
python scripts/mcp-client.py call -s "npx -y @upstash/context7-mcp" \
  -t query-docs \
  -p '{"libraryId": "/vercel/next.js", "query": "how does routing work"}'

# Get MongoDB aggregation examples
python scripts/mcp-client.py call -s "npx -y @upstash/context7-mcp" \
  -t query-docs \
  -p '{"libraryId": "/mongodb/docs", "query": "aggregation pipeline examples"}'
```

<details>
<summary>Full Schema</summary>

```json
{
  "type": "object",
  "properties": {
    "libraryId": {
      "type": "string",
      "description": "Exact Context7-compatible library ID (e.g., '/mongodb/docs', '/vercel/next.js') retrieved from 'resolve-library-id' or directly from user query."
    },
    "query": {
      "type": "string",
      "description": "The question or task you need help with. Be specific and include relevant details."
    }
  },
  "required": ["libraryId", "query"]
}
```
</details>

## Usage Patterns

### Pattern 1: Unknown Library

When you don't know the exact library ID:

```bash
# Step 1: Resolve library name
python scripts/mcp-client.py call -s "npx -y @upstash/context7-mcp" \
  -t resolve-library-id -p '{"query": "middleware", "libraryName": "express"}'

# Step 2: Use returned ID to fetch docs
python scripts/mcp-client.py call -s "npx -y @upstash/context7-mcp" \
  -t query-docs \
  -p '{"libraryId": "/expressjs/express", "query": "how to use middleware"}'
```

### Pattern 2: Known Library ID

When you know the library ID:

```bash
# Direct fetch (skip resolve step)
python scripts/mcp-client.py call -s "npx -y @upstash/context7-mcp" \
  -t query-docs \
  -p '{"libraryId": "/reactjs/react.dev", "query": "useState examples"}'
```

### Pattern 3: Using the Shell Pipeline (Recommended)

For token-efficient documentation fetching:

```bash
# Code examples (default, 60-70% token savings)
bash scripts/fetch-docs.sh --library react --topic useState --content-type examples

# Multiple content types
bash scripts/fetch-docs.sh --library react --topic useState --content-type examples,api-ref

# With verbose output to see savings
bash scripts/fetch-docs.sh --library react --topic useState --content-type examples --verbose
```

**Content types available:** `examples`, `api-ref`, `setup`, `concepts`, `migration`, `troubleshooting`, `patterns`, `all`

## Common Library IDs

Quick reference for popular libraries:

| Library | Context7 ID |
|---------|-------------|
| React | `/reactjs/react.dev` or `/websites/react_dev` |
| Next.js | `/vercel/next.js` |
| Express | `/expressjs/express` |
| MongoDB | `/mongodb/docs` |
| Prisma | `/prisma/docs` |
| Vue | `/vuejs/docs` |
| Svelte | `/sveltejs/svelte.dev` |
| FastAPI | `/tiangolo/fastapi` |
| Django | `/django/docs` |

## Error Handling

### Error Codes

| Error Code | Meaning | Call Cost | Action |
|------------|---------|-----------|--------|
| `[LIBRARY_NOT_FOUND]` | Library name didn't resolve | 1 call | Try spelling variations |
| `[LIBRARY_MISMATCH]` | Resolved to wrong library | 1 call | Use --library-id directly |
| `[INVALID_LIBRARY_ID]` | Bad ID format | 0 calls | Fix to `/org/project` format |
| `[EMPTY_RESULTS]` | No docs for query | 1-2 calls | Broaden topic or --content-type all |
| `[RATE_LIMIT_ERROR]` | Context7 limit hit | N/A | Wait, check API key |
| `[FETCH_FAILED_AFTER_RETRIES]` | Network issues | 0 calls | Safe to retry |

### Retry Logic

- **Infrastructure failures** (timeout, network): Auto-retries with exponential backoff (2s, 5s, 10s)
- **API errors** (rate limit, auth): No retry (would waste call budget)

---

## API Key Setup

Required for reliable access. Get free key at [context7.com/dashboard](https://context7.com/dashboard).

### Setup Methods

**Method 1: Config File** (Recommended - persistent)
```bash
# User-level (applies to all projects)
echo "CONTEXT7_API_KEY=ctx7sk_your_key" > ~/.context7.env

# Project-level (add to .gitignore!)
echo "CONTEXT7_API_KEY=ctx7sk_your_key" > .context7.env
```

**Method 2: Environment Variable**
```bash
# macOS/Linux
export CONTEXT7_API_KEY="ctx7sk_your_key"

# Windows (PowerShell)
$env:CONTEXT7_API_KEY = "ctx7sk_your_key"
```

### Check Status

```bash
bash scripts/fetch-docs.sh --api-status
```

### Priority Order

1. `CONTEXT7_API_KEY` environment variable (highest)
2. `.context7.env` in current directory (project-level)
3. `~/.context7.env` in home directory (user-level)

---

## Parameters Reference

```bash
bash scripts/fetch-docs.sh [OPTIONS]
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `--library <name>` | One of these | Library name (uses 2 calls for resolution) |
| `--library-id <id>` | required | Direct ID like `/vercel/next.js` (uses 1 call) |
| `--topic <topic>` | Recommended | Feature to focus on |
| `--content-type <types>` | Optional | Comma-separated: examples, api-ref, setup, concepts, migration, troubleshooting, patterns, all |
| `--max-items <num>` | Optional | Max items per type (default: 5) |
| `--verbose` | Optional | Show token savings stats |
| `--api-status` | Optional | Check API key configuration |

---

## Tips

1. **Save API Calls**: Use `--library-id` when you know the ID (saves 1 of your 3 calls)
2. **Specific Queries**: More specific queries yield better results
3. **Use Shell Pipeline**: `fetch-docs.sh` provides 60-90% token savings through content-type filtering
4. **Match Content Type**: Use `examples` for code, `api-ref` for parameters, `setup` for installation
5. **Combine Types**: Use comma-separated types like `examples,api-ref` for comprehensive output
6. **Fallback**: If no results, try `--content-type all` or broader topics
