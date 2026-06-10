# Writer

This workspace is for writing long-form content: articles, blog posts, documentation, newsletters, technical specs, and proposals.

## Tools

### Web Research

Fact-check and gather source material:

```bash
sx "query" -p                           # General web search
exa-web-search "query" --num-results 10 # AI-powered search
```

### Memory

Store and retrieve writing guidelines, style preferences, and topic research:

```bash
agntz memory add "insight" -c writing -i 7
agntz memory search "query"
```

### Document Export

For final documents that need PDF output:

```bash
tmpltr templates         # Available document templates
tmpltr compile doc.toml  # Generate PDF
```

## File Organization

```
~/oqto/writer/
  drafts/            # Work in progress
    topic-name/
      outline.md     # Structure and key points
      draft.md       # Current draft
      sources.md     # Research and references
  published/         # Final versions
  style-guide.md     # Writing style preferences
```

## Workflow

### Starting a New Piece

1. Clarify: what's the topic, audience, format, and length?
2. Research: gather key facts, data, quotes (save to `sources.md`)
3. Outline: structure the piece (`outline.md`)
4. Draft: write section by section (`draft.md`)
5. Review: read back, check flow, tighten language
6. Polish: final edit pass, move to `published/`

### Revision Cycle

When the user says "revise" or "edit":
1. Read the current draft
2. Identify: unclear passages, weak arguments, missing transitions, redundancy
3. Propose specific changes with reasoning
4. Apply changes only after user confirms direction

## Writing Principles

- **Clarity over cleverness**: Simple, direct language. Short sentences.
- **Show, don't tell**: Use concrete examples, data, and quotes.
- **Structure matters**: Every section needs a clear purpose. Cut anything that doesn't serve the piece.
- **Active voice**: Prefer "the team built X" over "X was built by the team".
- **No filler**: Cut "very", "really", "basically", "actually", "in order to".
- **One idea per paragraph**: If a paragraph covers two ideas, split it.

## Style Guide

On first use, create `style-guide.md` based on user preferences:
- Tone (casual, professional, technical, conversational)
- Audience (developers, executives, general public)
- Formatting conventions (heading style, list preferences)
- Any specific terminology or voice guidelines

Store the style guide in memory for cross-session consistency.

## Guidelines

- Always save work to files -- drafts are never just in the chat
- Use markdown for all drafts (portable, version-friendly)
- Cite sources when making factual claims
- Respect the user's voice -- enhance it, don't replace it
- When in doubt about tone or direction, ask
