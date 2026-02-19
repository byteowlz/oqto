#!/usr/bin/env bash
# sync-pi-config.sh - Sync Pi configuration files to user home directories
#
# Updates models.json, settings.json, and AGENTS.md for platform users.
# Useful when provider configuration changes or when a user's Pi config
# needs to be reset/updated to match the platform defaults.
#
# Operations:
#   - models.json:  Regenerated from EAVS provider catalog
#   - settings.json: Synced from a reference template (preserves user overrides unless --force)
#   - AGENTS.md:    Synced from onboarding templates
#
# Usage:
#   sync-pi-config.sh --all                     Sync all configs for all users
#   sync-pi-config.sh --user alice              Sync all configs for alice
#   sync-pi-config.sh --models --all            Only sync models.json
#   sync-pi-config.sh --settings --all          Only sync settings.json
#   sync-pi-config.sh --agents --all            Only sync AGENTS.md
#   sync-pi-config.sh --force --user alice      Overwrite user customizations

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

TARGET_USER=""
TARGET_ALL=false
DRY_RUN=false
FORCE=false

# What to sync (all by default)
SYNC_MODELS=false
SYNC_SETTINGS=false
SYNC_AGENTS=false
SYNC_EXPLICIT=false

# Reference files
REFERENCE_SETTINGS=""
REFERENCE_AGENTS=""

usage() {
    cat <<EOF
${BOLD}sync-pi-config${RESET} - Sync Pi configuration to users

${BOLD}USAGE${RESET}
    sync-pi-config.sh [options]

${BOLD}OPTIONS${RESET}
    --user, -u <name>       Target a specific user
    --all, -a               Target all active users
    --models                Only sync models.json (from EAVS catalog)
    --settings              Only sync settings.json
    --agents                Only sync AGENTS.md
    --force                 Overwrite user customizations (default: skip if exists)
    --reference-settings <f> Use a custom settings.json as reference
    --reference-agents <f>   Use a custom AGENTS.md as reference
    --dry-run               Show what would be done
    -h, --help              Show this help

${BOLD}NOTES${RESET}
    When no --models/--settings/--agents flag is given, all are synced.
    
    models.json is always regenerated from the live EAVS catalog.
    settings.json is only written if missing (unless --force).
    AGENTS.md is only written if missing (unless --force).

${BOLD}EXAMPLES${RESET}
    sync-pi-config.sh --all                    # Full sync for everyone
    sync-pi-config.sh --models --all           # Just update model catalog
    sync-pi-config.sh --force --user alice     # Reset alice's config
    sync-pi-config.sh --settings --reference-settings ./custom-settings.json --all

EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user|-u)    TARGET_USER="$2"; shift 2 ;;
        --all|-a)     TARGET_ALL=true; shift ;;
        --models)     SYNC_MODELS=true; SYNC_EXPLICIT=true; shift ;;
        --settings)   SYNC_SETTINGS=true; SYNC_EXPLICIT=true; shift ;;
        --agents)     SYNC_AGENTS=true; SYNC_EXPLICIT=true; shift ;;
        --force)      FORCE=true; shift ;;
        --dry-run)    DRY_RUN=true; shift ;;
        --reference-settings) REFERENCE_SETTINGS="$2"; shift 2 ;;
        --reference-agents)   REFERENCE_AGENTS="$2"; shift 2 ;;
        -h|--help)    usage ;;
        *)            log_error "Unknown option: $1"; usage ;;
    esac
done

# If no explicit sync targets, sync everything
if ! $SYNC_EXPLICIT; then
    SYNC_MODELS=true
    SYNC_SETTINGS=true
    SYNC_AGENTS=true
fi

# --- Default reference files --------------------------------------------------

# Build a default settings.json if no reference provided
get_default_settings() {
    local config
    config="$(find_config 2>/dev/null)" || true

    # Read defaults from oqto config
    local default_provider default_model
    default_provider="$(read_toml_value "${config:-/dev/null}" "pi" "default_provider" 2>/dev/null)" || default_provider="openai"
    default_model="$(read_toml_value "${config:-/dev/null}" "pi" "default_model" 2>/dev/null)" || default_model="gpt-4o"

    cat <<SETTINGS
{
  "\$schema": "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/pi-agent/settings.schema.json",
  "defaultModel": "$default_model",
  "defaultProvider": "$default_provider",
  "retry": {
    "enabled": true
  }
}
SETTINGS
}

# Get AGENTS.md content from templates or embedded default
get_default_agents_md() {
    # Check templates directory from config
    local config
    config="$(find_config 2>/dev/null)" || true

    local templates_path
    templates_path="$(read_toml_value "${config:-/dev/null}" "templates" "repo_path" 2>/dev/null)" || true

    # Try loading from templates directory
    if [[ -n "$templates_path" ]]; then
        local agents_file="$templates_path/AGENTS.md"
        if [[ -f "$agents_file" ]]; then
            cat "$agents_file"
            return 0
        fi
    fi

    # Fallback: embedded default
    cat <<'AGENTS'
# Agent Configuration

## Tools Available

- File reading and writing
- Shell command execution
- Code search and navigation
- Git operations

## Guidelines

- Always read files before editing them
- Run tests after making changes
- Commit work with descriptive messages
- Ask for clarification when requirements are ambiguous
AGENTS
}

# --- Sync functions -----------------------------------------------------------

sync_models_for_user() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    local eavs_base
    eavs_base="$(get_eavs_url)"

    local providers
    providers="$(curl -sf "${eavs_base}/providers/detail" 2>/dev/null)" || {
        log_fail "Could not fetch EAVS provider details from $eavs_base"
        return 1
    }

    # Generate models.json (same logic as eavs-provision)
    local models_json
    models_json="$(echo "$providers" | jq --arg base "$eavs_base" '
    {
        providers: (
            [.[] | select(.name != "default" and .pi_api != null)] |
            map({
                key: ("eavs-" + .name),
                value: {
                    baseUrl: ($base + "/" + .name + "/v1"),
                    api: .pi_api,
                    apiKey: "EAVS_API_KEY",
                    models: [.models[] | {
                        id: .id,
                        name: (if .name == "" then .id else .name end),
                        reasoning: .reasoning,
                        input: (if (.input | length) == 0 then ["text"] else .input end),
                        contextWindow: .context_window,
                        maxTokens: .max_tokens,
                        cost: {
                            input: .cost.input,
                            output: .cost.output,
                            cacheRead: .cost.cache_read,
                            cacheWrite: 0
                        }
                    }]
                }
            }) |
            from_entries
        )
    }')"

    if $DRY_RUN; then
        log_step "[dry-run] Would write $home/.pi/agent/models.json"
        return 0
    fi

    write_file_as_user "$linux_username" "$home/.pi/agent" "models.json" "$models_json"
    log_step "Updated models.json"
}

sync_settings_for_user() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    local target="$home/.pi/agent/settings.json"

    # Skip if exists and not forcing
    if [[ -f "$target" ]] && ! $FORCE; then
        log_step "settings.json already exists (use --force to overwrite)"
        return 0
    fi

    local content
    if [[ -n "$REFERENCE_SETTINGS" && -f "$REFERENCE_SETTINGS" ]]; then
        content="$(cat "$REFERENCE_SETTINGS")"
    else
        content="$(get_default_settings)"
    fi

    if $DRY_RUN; then
        log_step "[dry-run] Would write $target"
        return 0
    fi

    write_file_as_user "$linux_username" "$home/.pi/agent" "settings.json" "$content"
    log_step "Updated settings.json"
}

sync_agents_for_user() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    local target="$home/.pi/agent/AGENTS.md"

    # Skip if exists and not forcing
    if [[ -f "$target" ]] && ! $FORCE; then
        log_step "AGENTS.md already exists (use --force to overwrite)"
        return 0
    fi

    local content
    if [[ -n "$REFERENCE_AGENTS" && -f "$REFERENCE_AGENTS" ]]; then
        content="$(cat "$REFERENCE_AGENTS")"
    else
        content="$(get_default_agents_md)"
    fi

    if $DRY_RUN; then
        log_step "[dry-run] Would write $target"
        return 0
    fi

    write_file_as_user "$linux_username" "$home/.pi/agent" "AGENTS.md" "$content"
    log_step "Updated AGENTS.md"
}

# Combined sync for a user
sync_user() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    log_info "Syncing Pi config for ${BOLD}${linux_username}${RESET}"

    local errors=0

    if $SYNC_MODELS; then
        sync_models_for_user "$user_id" "$linux_username" "$home" || ((errors++)) || true
    fi

    if $SYNC_SETTINGS; then
        sync_settings_for_user "$user_id" "$linux_username" "$home" || ((errors++)) || true
    fi

    if $SYNC_AGENTS; then
        sync_agents_for_user "$user_id" "$linux_username" "$home" || ((errors++)) || true
    fi

    if [[ $errors -eq 0 ]]; then
        log_ok "Pi config synced for $linux_username"
    else
        log_fail "$errors component(s) failed for $linux_username"
        return 1
    fi
}

# --- Main ---------------------------------------------------------------------

if [[ -z "$TARGET_USER" ]] && ! $TARGET_ALL; then
    log_error "Specify --user <name> or --all"
    exit 1
fi

for_each_user sync_user ${TARGET_USER:+--user "$TARGET_USER"} ${TARGET_ALL:+--all}
