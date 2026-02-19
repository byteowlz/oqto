# oqto-admin - Administrative Scripts

Command-line tools for managing Oqto platform users, EAVS keys, Pi configuration, skills, and bootstrap documents.

## Requirements

- Must be run as the server admin user (the user running `oqto`)
- Required tools: `jq`, `curl`, `sqlite3`
- For multi-user operations: `sudo` access or `oqto-usermgr` daemon running

## Quick Start

```bash
# Show all available commands
./scripts/admin/oqto-admin help

# Or use just recipes
just admin help

# Check current user provisioning status
just admin-status

# Full sync for all users (eavs + pi config + skills)
just admin-sync-all
```

## Commands

### eavs-provision

Provision or re-provision EAVS API keys and models.json for users.

```bash
# Provision EAVS for a single user
just admin-eavs --user alice

# Provision EAVS for all active users
just admin-eavs --all

# Only regenerate models.json (no key changes)
just admin-eavs --sync-models --all

# Rotate (revoke old + create new) a user's EAVS key
just admin-eavs --rotate --user bob

# Show EAVS key status overview
just admin-eavs --status
```

**What it does:**
1. Creates a new EAVS virtual key bound to the user via `oauth_user`
2. Writes `~/.config/oqto/eavs.env` with the key and EAVS URL
3. Regenerates `~/.pi/agent/models.json` from the live EAVS provider catalog

### sync-pi-config

Sync Pi configuration files (models.json, settings.json, AGENTS.md) to user home directories.

```bash
# Full sync for all users
just admin-sync-pi --all

# Only update model catalog
just admin-sync-pi --models --all

# Only update settings.json
just admin-sync-pi --settings --all

# Force overwrite existing user customizations
just admin-sync-pi --force --user alice

# Use a custom reference settings file
just admin-sync-pi --settings --reference-settings ./my-settings.json --all
```

**Behavior:**
- `models.json` is always regenerated from the live EAVS catalog
- `settings.json` is only written if missing (unless `--force`)
- `AGENTS.md` is only written if missing (unless `--force`)

### manage-skills

Install, update, remove, and list Pi skills for platform users.

```bash
# List available skills in the source directory
just admin-skills --list

# List skills installed for a specific user
just admin-skills --list-user --user alice

# Install a specific skill for all users
just admin-skills --install canvas-design --all

# Install all available skills for a user
just admin-skills --install-all --user alice

# Remove a skill from all users
just admin-skills --remove old-skill --all

# Update existing skills to latest version
just admin-skills --update --all

# Use a custom skills source directory
just admin-skills --source ~/custom-skills --install my-skill --all
```

**Skill source resolution (first match wins):**
1. `--source <path>` argument
2. `/usr/share/oqto/skills/`
3. `~/.pi/agent/skills/` (admin user's skills)

### manage-templates

Manage onboarding/bootstrap document templates (AGENTS.md, ONBOARD.md, PERSONALITY.md, USER.md).

```bash
# Sync templates from remote git repo
just admin-templates --sync

# List available templates and presets
just admin-templates --list

# Deploy bootstrap docs to all users
just admin-templates --deploy --all

# Deploy with a specific preset
just admin-templates --deploy --user alice --preset developer

# Force overwrite existing bootstrap docs
just admin-templates --deploy --all --force

# Show contents of a template
just admin-templates --show AGENTS.md

# Verify template setup is healthy
just admin-templates --check
```

**Available presets:**
- `developer` - Technical users familiar with AI coding
- `beginner` - New users who need more guidance
- `enterprise` - Work-focused setup, skip personal customization

### sync-all

Run a full provisioning sync combining all the above operations.

```bash
# Full sync for all active users
just admin-sync-all

# Full sync for a specific user
just admin-sync-all --user alice

# Skip specific steps
just admin-sync-all --skip-eavs
just admin-sync-all --skip-skills

# Force overwrite user customizations
just admin-sync-all --force

# Preview changes
just admin-sync-all --dry-run
```

**Steps executed:**
1. Sync onboarding templates from remote repo
2. Verify/provision EAVS keys (models.json + eavs.env)
3. Sync Pi configuration (settings.json, AGENTS.md)
4. Update installed skills from source

### user-status

Show detailed provisioning status for all users.

```bash
# Show status for all users
just admin-status

# Show status for a specific user
just admin-status --user alice

# Output as JSON
just admin-status --json
```

**Checks:**
- Linux user/UID mapping
- EAVS key + eavs.env
- models.json, settings.json, AGENTS.md
- Installed skills count
- Bootstrap documents (ONBOARD.md, PERSONALITY.md, USER.md)
- Runner socket status

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `OQTO_SERVER_URL` | Backend API URL | `http://localhost:8080/api` |
| `OQTO_ADMIN_SOCKET` | Admin socket path | `/run/oqto/oqtoctl.sock` |
| `OQTO_CONFIG` | Config file path | Auto-detected |
| `OQTO_DB` | Database path | Auto-detected |
| `EAVS_MASTER_KEY` | EAVS admin key | Read from config |
| `EAVS_URL` | EAVS base URL | Read from config |

## Architecture

```
oqto-admin (dispatcher)
  |
  +-- lib.sh (shared functions: user listing, file ops, config parsing)
  |
  +-- eavs-provision.sh    (EAVS key management + models.json)
  +-- sync-pi-config.sh    (Pi settings + AGENTS.md)
  +-- manage-skills.sh     (skill installation/updates)
  +-- manage-templates.sh  (bootstrap document templates)
  +-- sync-all.sh          (orchestrates all of the above)
  +-- user-status.sh       (read-only status reporting)
```

All scripts:
- Read user data directly from the oqto SQLite database
- Use `sudo` or direct file operations depending on the current user
- Support `--dry-run` for previewing changes
- Are idempotent and safe to re-run
