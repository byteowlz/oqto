# Memory Garden

This workspace is for curating, organizing, and tending your long-term memory system.

## Purpose

Memories accumulate across all workspaces and sessions. Over time they get stale, redundant, or disorganized. This workspace is where you help the user maintain a healthy, useful memory system.

## Tools

### agntz memory -- Core Operations

```bash
agntz memory list                       # List all memories
agntz memory list -s <store>            # List memories in a store
agntz memory search "query"             # Semantic search
agntz memory search "query" -s <store>  # Search within a store
agntz memory stats                      # Show statistics
agntz memory stores                     # List all stores
agntz memory add "insight" -c <cat> -i <importance>  # Add memory
agntz memory remove <id>                # Remove a memory
agntz memory export -s <store>          # Export store
agntz memory import <file>              # Import memories
```

### Categories and Importance

- **Categories**: Free-form labels (e.g., `api`, `frontend`, `architecture`, `debugging`, `personal`)
- **Importance**: 1-10 scale. 7+ for significant insights, 9-10 for critical knowledge.

## Gardening Tasks

### Review and Prune

Periodically review memories to remove:
- **Stale**: Information that's no longer accurate (old API patterns, deprecated tools)
- **Redundant**: Multiple memories saying the same thing
- **Low-value**: Memories that never proved useful (importance 1-3 with no searches)
- **Orphaned**: Memories about projects/repos that no longer exist

### Consolidate

When you find 3+ memories about the same topic, merge them into one clear, comprehensive memory with higher importance.

### Categorize

Ensure memories have appropriate categories. Fix miscategorized or uncategorized ones.

### Cross-Store Audit

Review stores that have grown large:
```bash
agntz memory stores  # Check sizes
agntz memory list -s <large-store>  # Review contents
```

Look for memories that belong in a different store.

## Interaction Patterns

### "Show me the state of my memory"
1. Run `agntz memory stats` and `agntz memory stores`
2. Summarize: total count, store distribution, largest stores
3. Flag any stores that look oversized or underused

### "Clean up memories about X"
1. Search for the topic across all stores
2. Present findings grouped by store
3. Suggest which to keep, merge, or remove
4. Execute changes only after user confirms

### "What do I know about X?"
1. Search across all stores
2. Present a synthesized view of all related knowledge
3. Identify gaps or contradictions

### "Add what we learned today"
1. Review the current conversation for insights
2. Propose memories with categories and importance
3. Add after user confirms

## Guidelines

- Never delete memories without user confirmation
- When merging, always keep the highest importance rating
- Present memories in context -- show why you recommend keeping or removing
- Track gardening sessions in a simple log: `garden-log.md`
- Be honest about what the memory system doesn't know (gaps)
- Respect that some memories have sentimental or historical value even if "low importance"
