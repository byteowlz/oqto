---
name: deep-research
description: This skill should be used when users request comprehensive, in-depth research on a topic that requires detailed analysis similar to an academic journal or whitepaper. The skill conducts multi-phase research using web search and content analysis, producing a detailed markdown report with citations.
license: MIT
---

# Deep Research

Conducts comprehensive research on complex topics, producing detailed reports similar to academic journals or whitepapers.

## When to Use

Use when the user requests:
- In-depth research on a complex topic
- A comprehensive report or analysis
- Research requiring multiple sources and synthesis
- Deep investigation to academic or whitepaper standards

Do NOT use for:
- Simple fact-finding queries
- Single-source lookups
- Code-only research within repositories

## Available Tools

You have these tools for research:
- **sx** (via bash): Web search CLI. Usage: `sx "query" -p` returns top results with snippets
- **exa-web-search** (via bash): AI-powered search. Usage: `exa-web-search "query" --num-results 10`
- **exa-code-context** (via bash): Code/technical search. Usage: `exa-code-context "query"`
- **bash**: Run commands, fetch URLs with `curl`
- **read/write**: Read and write files
- **Todo**: Track research progress

**You do NOT have a Task or subagent tool.** Do all research yourself sequentially.

## Research Process

### Phase 1: Scope Definition (1-2 questions max)

Ask the user briefly:
1. What specifically do they want to understand?
2. Any particular focus areas or exclusions?

If the request is already clear (e.g., "deep research on PLM and AI"), skip the interview and start immediately.

### Phase 2: Initial Reconnaissance

Map the research landscape with 3-5 broad searches:

```bash
sx "PLM artificial intelligence state of the art 2025" -p
sx "AI product lifecycle management market leaders" -p
sx "generative AI manufacturing CAD design" -p
```

From the results:
1. Identify 8-12 key subtopics/threads
2. Note the most promising sources and authoritative voices
3. Create `research_plan.md` listing all threads

### Phase 3: Deep Research (Sequential)

Work through each research thread one at a time. For each thread:

1. Run 2-3 targeted searches
2. Fetch key pages for detailed content where needed
3. Save structured notes to `research_notes/[subtopic-slug].md`

Use Todo to track progress through threads:

```
TodoWrite: [
  { content: "Thread 1: Market landscape", status: "in_progress" },
  { content: "Thread 2: Technical capabilities", status: "pending" },
  ...
]
```

**Research note format** (save to `research_notes/[subtopic].md`):

```markdown
# [Subtopic Title]

## Summary
[2-3 paragraph summary of key findings]

## Key Findings
- [Bullet points of important facts, data, techniques]

## Sources
1. [URL] - [Brief description]
2. [URL] - [Brief description]

## Notable Quotes
> "[Relevant quote]" - Source

## Gaps and Conflicts
- [Any conflicting information found]
```

### Phase 4: Report Synthesis

After all threads are researched, read all notes from `research_notes/` and write the final report.

**Report structure** (write to `[topic]-report.md`):

```markdown
# [Topic]: Comprehensive Research Report

## Executive Summary
[2-3 paragraphs: what was researched, key findings, main conclusions]

## [Section 1 - adapt to topic]
...

## [Section N - adapt to topic]
...

## Critical Analysis
[Strengths, weaknesses, gaps, conflicting viewpoints]

## Conclusions
[Key takeaways, recommendations, future outlook]

## References
[1] Author/Site. "Title." URL. Accessed [date].
[2] ...
```

**Sources bibliography** (write to `[topic]-sources.md`):

```markdown
# Research Sources for [Topic]

## [1] Source Title
- **URL**: [url]
- **Type**: [Academic paper / Industry report / Blog / Documentation]
- **Key Points**: [bullet points]
- **Relevance**: [why this source matters]
```

### Phase 5: Deliver

1. Tell the user where the files are:
   - `[topic]-report.md` -- main report
   - `[topic]-sources.md` -- bibliography
   - `research_notes/` -- raw research (can be deleted)
2. Give a brief verbal summary of the top findings
3. Offer to expand on any section

## File Structure

```
./[topic-directory]/
  research_plan.md
  research_notes/
    subtopic-1.md
    subtopic-2.md
    ...
  [topic]-report.md
  [topic]-sources.md
```

## Research Quality Guidelines

- Prioritize authoritative, recent sources (last 1-2 years for fast-moving fields)
- Cross-reference claims across multiple sources
- Note conflicting information -- do not hide disagreements
- Distinguish facts from expert opinions from speculation
- Be transparent about limitations in available information
- Use numbered citations [1], [2], etc. throughout the report
- Include comparison tables where appropriate

## Common Patterns

**Comparative research** (comparing technologies/solutions): dedicate threads to each option plus cross-cutting concerns (cost, performance, adoption). Use tables in the report.

**Technical deep-dive**: threads for fundamentals, implementation details, case studies, limitations. Structure report from basics to advanced.

**Market/landscape survey**: threads for major players, emerging players, trends, analyst perspectives. Categorize and evaluate.

**Historical/evolution research**: threads for key eras or events. Build a timeline in the report.
