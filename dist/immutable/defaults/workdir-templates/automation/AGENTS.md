# Automation

This workspace is for building scripts, scheduled tasks, and small automation workflows.

## Tools

### skdlr -- Task Scheduler

Schedule recurring or one-time tasks with native OS integration (systemd timers on Linux):

```bash
skdlr add "backup" --command "rsync -a ~/docs/ /backup/docs/" --cron "0 2 * * *"
skdlr add "cleanup" --command "/home/user/scripts/cleanup.sh" --interval 1h
skdlr list                    # List all schedules
skdlr show <name>             # Show schedule details
skdlr logs <name>             # View execution history
skdlr status                  # Status overview
skdlr next                    # Show upcoming runs
skdlr run <name>              # Trigger immediate run
skdlr enable/disable <name>   # Toggle schedules
skdlr doctor                  # Health check
```

### Scripting

Write scripts in bash or Python (via uv):

**Bash:**
```bash
#!/usr/bin/env bash
set -euo pipefail
```

**Python:**
```bash
uv init              # New Python project
uv add <package>     # Add dependency
uv run script.py     # Run script
```

## File Organization

```
~/oqto/automation/
  scripts/           # Standalone scripts
  schedules/         # Documentation for scheduled tasks
  logs/              # Script output logs (if needed)
  README.md          # Index of all automations
```

## Workflow

1. User describes what they want automated
2. Write the script, test it manually
3. If it should run on a schedule, set up via skdlr
4. Document in README.md what each automation does

## Guidelines

- Scripts must be idempotent where possible (safe to re-run)
- Always use `set -euo pipefail` in bash scripts
- Use `#!/usr/bin/env bash` shebang
- Log output to stdout/stderr (skdlr captures it)
- Test scripts manually before scheduling
- Add error handling -- scripts run unattended
- Document what each script does, its schedule, and any dependencies
- For Python scripts, use uv for dependency management
- Keep scripts focused -- one task per script
- Use environment variables for configuration, not hardcoded paths
