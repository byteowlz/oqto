#!/usr/bin/env bash
# manage-templates.sh - Manage onboarding/bootstrap document templates
#
# Templates are the bootstrap documents that configure each user's agent:
#   - AGENTS.md:       Agent configuration and guidelines
#   - ONBOARD.md:      Onboarding flow instructions
#   - PERSONALITY.md:  Agent personality/tone configuration
#   - USER.md:         User profile and preferences
#
# Templates are synced from a git repo or local directory configured in
# the [templates] section of config.toml.
#
# Usage:
#   manage-templates.sh --sync             Sync templates from remote source
#   manage-templates.sh --list             List available templates
#   manage-templates.sh --deploy --all     Deploy templates to all users
#   manage-templates.sh --deploy --user x  Deploy templates to a specific user
#   manage-templates.sh --show <name>      Show contents of a template

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

MODE=""
TARGET_USER=""
TARGET_ALL=false
DRY_RUN=false
FORCE=false
TEMPLATE_NAME=""
PRESET=""

usage() {
    cat <<EOF
${BOLD}manage-templates${RESET} - Manage onboarding/bootstrap document templates

${BOLD}USAGE${RESET}
    manage-templates.sh <action> [options]

${BOLD}ACTIONS${RESET}
    --sync                  Sync templates from remote git repo
    --list                  List available templates and presets
    --deploy                Deploy bootstrap docs to user(s)
    --show <name>           Show contents of a specific template
    --check                 Verify template setup is healthy

${BOLD}OPTIONS${RESET}
    --user, -u <name>       Target a specific user
    --all, -a               Target all active users
    --preset <name>         Use a specific preset (developer, beginner, enterprise)
    --force                 Overwrite existing bootstrap docs
    --dry-run               Show what would be done
    -h, --help              Show this help

${BOLD}TEMPLATE SOURCES${RESET}
    Templates are loaded from (in priority order):
    1. [templates].repo_path in config.toml (local path)
    2. Git repo configured in [templates].repo_url
    3. Embedded defaults (compiled into oqto binary)

${BOLD}BOOTSTRAP DOCUMENTS${RESET}
    AGENTS.md        Agent configuration and guidelines
    ONBOARD.md       Onboarding flow instructions
    PERSONALITY.md   Agent personality/tone configuration
    USER.md          User profile and preferences

${BOLD}EXAMPLES${RESET}
    manage-templates.sh --sync
    manage-templates.sh --list
    manage-templates.sh --deploy --all
    manage-templates.sh --deploy --user alice --preset developer
    manage-templates.sh --deploy --all --force
    manage-templates.sh --show AGENTS.md
    manage-templates.sh --check

EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --sync)       MODE="sync"; shift ;;
        --list)       MODE="list"; shift ;;
        --deploy)     MODE="deploy"; shift ;;
        --show)       MODE="show"; TEMPLATE_NAME="$2"; shift 2 ;;
        --check)      MODE="check"; shift ;;
        --user|-u)    TARGET_USER="$2"; shift 2 ;;
        --all|-a)     TARGET_ALL=true; shift ;;
        --preset)     PRESET="$2"; shift 2 ;;
        --force)      FORCE=true; shift ;;
        --dry-run)    DRY_RUN=true; shift ;;
        -h|--help)    usage ;;
        *)            log_error "Unknown option: $1"; usage ;;
    esac
done

# --- Template source resolution -----------------------------------------------

get_templates_dir() {
    local config
    config="$(find_config 2>/dev/null)" || true

    # Check config for local path
    local repo_path
    repo_path="$(read_toml_value "${config:-/dev/null}" "templates" "repo_path" 2>/dev/null)" || true

    if [[ -n "$repo_path" && -d "$repo_path" ]]; then
        echo "$repo_path"
        return 0
    fi

    # Check for cached git repo
    local xdg_data="${XDG_DATA_HOME:-$HOME/.local/share}"
    local cache_dir="$xdg_data/oqto/onboarding-templates"
    if [[ -d "$cache_dir" ]]; then
        local subdir
        subdir="$(read_toml_value "${config:-/dev/null}" "templates" "subdirectory" 2>/dev/null)" || subdir="onboarding"
        local full_path="$cache_dir/$subdir"
        if [[ -d "$full_path" ]]; then
            echo "$full_path"
            return 0
        fi
        echo "$cache_dir"
        return 0
    fi

    # Default location
    echo "/usr/share/oqto/oqto-templates/onboarding"
}

get_templates_repo_url() {
    local config
    config="$(find_config 2>/dev/null)" || true

    local repo_url
    repo_url="$(read_toml_value "${config:-/dev/null}" "templates" "repo_url" 2>/dev/null)" || true

    echo "${repo_url:-https://github.com/byteowlz/oqto-templates}"
}

get_templates_branch() {
    local config
    config="$(find_config 2>/dev/null)" || true

    local branch
    branch="$(read_toml_value "${config:-/dev/null}" "templates" "branch" 2>/dev/null)" || true

    echo "${branch:-main}"
}

# --- Actions ------------------------------------------------------------------

do_sync() {
    local config
    config="$(find_config 2>/dev/null)" || true

    # Check if using local path (no sync needed)
    local repo_path
    repo_path="$(read_toml_value "${config:-/dev/null}" "templates" "repo_path" 2>/dev/null)" || true

    if [[ -n "$repo_path" && -d "$repo_path" ]]; then
        log_info "Templates configured as local path: $repo_path"
        log_info "No sync needed (local path is used directly)"
        return 0
    fi

    local repo_url branch cache_dir
    repo_url="$(get_templates_repo_url)"
    branch="$(get_templates_branch)"

    local xdg_data="${XDG_DATA_HOME:-$HOME/.local/share}"
    cache_dir="$xdg_data/oqto/onboarding-templates"

    log_info "Syncing templates from ${BOLD}${repo_url}${RESET} (branch: $branch)"

    if $DRY_RUN; then
        log_step "[dry-run] Would sync to $cache_dir"
        return 0
    fi

    if [[ ! -d "$cache_dir/.git" ]]; then
        log_step "Cloning templates repository..."
        mkdir -p "$(dirname "$cache_dir")"
        git clone --depth 1 --branch "$branch" "$repo_url" "$cache_dir" || {
            log_fail "Failed to clone templates repo"
            return 1
        }
    else
        log_step "Pulling latest changes..."
        (cd "$cache_dir" && git pull --ff-only) || {
            log_warn "git pull failed, trying fresh clone..."
            rm -rf "$cache_dir"
            git clone --depth 1 --branch "$branch" "$repo_url" "$cache_dir" || {
                log_fail "Failed to clone templates repo"
                return 1
            }
        }
    fi

    log_ok "Templates synced to $cache_dir"
}

do_list() {
    local templates_dir
    templates_dir="$(get_templates_dir)"

    log_info "Templates directory: ${BOLD}${templates_dir}${RESET}"
    echo ""

    if [[ ! -d "$templates_dir" ]]; then
        log_warn "Templates directory does not exist. Run --sync first."
        return 1
    fi

    # List template files
    echo -e "${BOLD}Templates:${RESET}"
    for f in "$templates_dir"/*.md; do
        [[ ! -f "$f" ]] && continue
        local name size
        name="$(basename "$f")"
        size="$(wc -c < "$f")"
        printf "  %-30s %s bytes\n" "$name" "$size"
    done

    # List i18n templates if present
    if [[ -d "$templates_dir/i18n" ]]; then
        echo ""
        echo -e "${BOLD}Translations:${RESET}"
        for lang_dir in "$templates_dir/i18n"/*/; do
            [[ ! -d "$lang_dir" ]] && continue
            local lang
            lang="$(basename "$lang_dir")"
            local file_count
            file_count="$(find "$lang_dir" -name "*.md" | wc -l)"
            printf "  %-30s %s files\n" "$lang" "$file_count"
        done
    fi

    # List presets
    echo ""
    echo -e "${BOLD}Presets:${RESET}"
    echo "  developer     Technical users familiar with AI coding"
    echo "  beginner      New users who need more guidance"
    echo "  enterprise    Work-focused setup, skip personal customization"
    echo ""
}

do_show() {
    local templates_dir
    templates_dir="$(get_templates_dir)"

    local target="$templates_dir/$TEMPLATE_NAME"
    if [[ ! -f "$target" ]]; then
        # Try with .md extension
        target="$templates_dir/${TEMPLATE_NAME}.md"
    fi

    if [[ ! -f "$target" ]]; then
        log_error "Template not found: $TEMPLATE_NAME"
        log_error "Checked: $templates_dir/$TEMPLATE_NAME"
        return 1
    fi

    echo -e "${DIM}--- $target ---${RESET}"
    cat "$target"
    echo -e "${DIM}--- end ---${RESET}"
}

deploy_to_user() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    log_info "Deploying bootstrap docs to ${BOLD}${linux_username}${RESET}"

    local templates_dir
    templates_dir="$(get_templates_dir)"

    if [[ ! -d "$templates_dir" ]]; then
        log_fail "Templates directory not found: $templates_dir"
        log_fail "Run 'manage-templates.sh --sync' first"
        return 1
    fi

    # Determine which template files to use (preset overrides)
    local agents_file="AGENTS.md"
    local onboard_file="ONBOARD.md"
    local personality_file="PERSONALITY.md"
    local user_file="USER.md"

    if [[ -n "$PRESET" ]]; then
        case "$PRESET" in
            developer)
                personality_file="PERSONALITY_TECHNICAL.md"
                ;;
            beginner)
                onboard_file="ONBOARD_BEGINNER.md"
                personality_file="PERSONALITY_FRIENDLY.md"
                ;;
            enterprise)
                onboard_file="ONBOARD_ENTERPRISE.md"
                ;;
            *)
                log_warn "Unknown preset: $PRESET (using defaults)"
                ;;
        esac
    fi

    local pi_dir="$home/.pi/agent"
    local errors=0

    # Deploy each template (skip if exists and not forcing)
    for template_pair in \
        "AGENTS.md:$agents_file" \
        "ONBOARD.md:$onboard_file" \
        "PERSONALITY.md:$personality_file" \
        "USER.md:$user_file"; do

        local target_name="${template_pair%%:*}"
        local source_name="${template_pair##*:}"
        local source_path="$templates_dir/$source_name"
        local target_path="$pi_dir/$target_name"

        # Skip if target exists and not forcing
        if [[ -f "$target_path" ]] && ! $FORCE; then
            log_step "$target_name already exists (use --force to overwrite)"
            continue
        fi

        # Check if source exists
        if [[ ! -f "$source_path" ]]; then
            # Fall back to default name
            source_path="$templates_dir/$target_name"
        fi

        if [[ ! -f "$source_path" ]]; then
            log_warn "Template not found: $source_name (skipping $target_name)"
            ((errors++)) || true
            continue
        fi

        if $DRY_RUN; then
            log_step "[dry-run] Would deploy $source_name -> $target_path"
            continue
        fi

        local content
        content="$(cat "$source_path")"
        write_file_as_user "$linux_username" "$pi_dir" "$target_name" "$content"
        log_step "Deployed $target_name"
    done

    if [[ $errors -eq 0 ]]; then
        log_ok "Bootstrap docs deployed for $linux_username"
    else
        log_warn "Deployed with $errors missing template(s)"
    fi
}

do_check() {
    log_info "Checking template configuration"
    echo ""

    local config
    config="$(find_config 2>/dev/null)" || true

    # Check config
    if [[ -n "$config" ]]; then
        log_ok "Config file: $config"
    else
        log_fail "No config file found"
    fi

    # Check templates directory
    local templates_dir
    templates_dir="$(get_templates_dir)"
    if [[ -d "$templates_dir" ]]; then
        local file_count
        file_count="$(find "$templates_dir" -name "*.md" -maxdepth 1 | wc -l)"
        log_ok "Templates directory: $templates_dir ($file_count .md files)"
    else
        log_fail "Templates directory not found: $templates_dir"
    fi

    # Check required templates exist
    for tmpl in AGENTS.md ONBOARD.md PERSONALITY.md USER.md; do
        if [[ -f "$templates_dir/$tmpl" ]]; then
            log_ok "  $tmpl"
        else
            log_warn "  $tmpl (missing)"
        fi
    done

    # Check user deployment status
    echo ""
    log_info "User deployment status:"

    local db
    db="$(find_db 2>/dev/null)" || {
        log_fail "Database not accessible"
        return 1
    }

    while IFS='|' read -r user_id username linux_username; do
        [[ -z "$linux_username" ]] && continue

        local home
        home="$(get_user_home "$linux_username" 2>/dev/null)" || continue

        local pi_dir="$home/.pi/agent"
        local has_agents="" has_onboard="" has_personality="" has_user=""

        [[ -f "$pi_dir/AGENTS.md" ]] && has_agents="Y"
        [[ -f "$pi_dir/ONBOARD.md" ]] && has_onboard="Y"
        [[ -f "$pi_dir/PERSONALITY.md" ]] && has_personality="Y"
        [[ -f "$pi_dir/USER.md" ]] && has_user="Y"

        printf "  %-15s  AGENTS:%-3s  ONBOARD:%-3s  PERSONALITY:%-3s  USER:%-3s\n" \
            "$username" "${has_agents:-N}" "${has_onboard:-N}" "${has_personality:-N}" "${has_user:-N}"

    done < <(sqlite3 -separator '|' "$db" \
        "SELECT id, username, linux_username FROM users WHERE is_active = 1 ORDER BY username")

    echo ""
}

# --- Main dispatch ------------------------------------------------------------

case "${MODE:-}" in
    sync)
        do_sync
        ;;
    list)
        do_list
        ;;
    show)
        if [[ -z "$TEMPLATE_NAME" ]]; then
            log_error "Specify template name: --show <name>"
            exit 1
        fi
        do_show
        ;;
    deploy)
        if [[ -z "$TARGET_USER" ]] && ! $TARGET_ALL; then
            log_error "Specify --user <name> or --all"
            exit 1
        fi
        for_each_user deploy_to_user ${TARGET_USER:+--user "$TARGET_USER"} ${TARGET_ALL:+--all}
        ;;
    check)
        do_check
        ;;
    "")
        log_error "No action specified"
        usage
        ;;
    *)
        log_error "Unknown mode: $MODE"
        usage
        ;;
esac
