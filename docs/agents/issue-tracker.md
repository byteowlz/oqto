# Issue tracker

Oqto uses `trx` as the source of truth for issues. Do not use GitHub Issues for repo work unless explicitly asked.

## Common workflow

```bash
trx ready                         # show unblocked open work
trx list                          # list issues
trx show <id>                     # inspect one issue
trx create "Title" -t task -p 2   # create issue
trx update <id> --status in_progress
trx close <id> -r "reason"
```

## Blocking

Use trx dependencies for blocking relationships:

```bash
trx dep block <issue> <blocker>
trx dep unblock <issue> <blocker>
trx dep tree <issue>
```

`trx ready` is the preferred queue for agent-pickable work because it filters out dependency-blocked issues.
