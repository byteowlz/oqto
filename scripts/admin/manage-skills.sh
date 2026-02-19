#!/usr/bin/env bash
# manage-skills.sh - Install, update, remove, and list Pi skills for users
#
# Skills are directories under ~/.pi/agent/skills/<skill-name>/
# containing at minimum a SKILL.md file.
#
# Skills can be sourced from:
#   - A shared skills repository (default: /usr/share/oqto/skills/)
#   - A local directory
#   - The admin user's own skill collection
#
# Usage:
#   manage-skills.sh --list                           List available skills
#   manage-skills.sh --list-user --user alice          List skills for alice
#   manage-skills.sh --install <skill> --all           Install a skill for all users
#   manage-skills.sh --install-all --all               Install all available skills
#   manage-skills.sh --remove <skill> --user alice     Remove a skill from alice
#   manage-skills.sh --update --all                    Update all skills for all users
#   manage-skills.sh --source /path/to/skills          Use a custom skills source

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

MODE=""         # list | list-user | install | install-all | remove | update
TARGET_USER=""
TARGET_ALL=false
DRY_RUN=false
SKILL_NAME=""
SKILLS_SOURCE=""

# Default skills source locations (checked in order)
DEFAULT_SKILLS_SOURCES=(
    "/usr/share/oqto/skills"
    "$HOME/.pi/agent/skills"
)

usage() {
    cat <<EOF
${BOLD}manage-skills${RESET} - Manage Pi skills for Oqto users

${BOLD}USAGE${RESET}
    manage-skills.sh <action> [options]

${BOLD}ACTIONS${RESET}
    --list                      List available skills in the source
    --list-user                 List skills installed for a user
    --install <name>            Install a specific skill
    --install-all               Install all available skills
    --remove <name>             Remove a specific skill
    --update                    Update existing skills to latest version

${BOLD}OPTIONS${RESET}
    --user, -u <name>           Target a specific user
    --all, -a                   Target all active users
    --source <path>             Skills source directory
    --dry-run                   Show what would be done
    -h, --help                  Show this help

${BOLD}SKILL SOURCE RESOLUTION${RESET}
    Skills are sourced from (first match wins):
    1. --source <path> argument
    2. /usr/share/oqto/skills/
    3. ~/.pi/agent/skills/ (admin user's skills)

${BOLD}EXAMPLES${RESET}
    manage-skills.sh --list
    manage-skills.sh --install canvas-design --all
    manage-skills.sh --install-all --user alice
    manage-skills.sh --remove old-skill --all
    manage-skills.sh --update --all
    manage-skills.sh --list-user --user alice
    manage-skills.sh --source ~/my-skills --install custom-skill --all

EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --list)         MODE="list"; shift ;;
        --list-user)    MODE="list-user"; shift ;;
        --install)      MODE="install"; SKILL_NAME="$2"; shift 2 ;;
        --install-all)  MODE="install-all"; shift ;;
        --remove)       MODE="remove"; SKILL_NAME="$2"; shift 2 ;;
        --update)       MODE="update"; shift ;;
        --user|-u)      TARGET_USER="$2"; shift 2 ;;
        --all|-a)       TARGET_ALL=true; shift ;;
        --source)       SKILLS_SOURCE="$2"; shift 2 ;;
        --dry-run)      DRY_RUN=true; shift ;;
        -h|--help)      usage ;;
        *)              log_error "Unknown option: $1"; usage ;;
    esac
done

# --- Skills source resolution -------------------------------------------------

find_skills_source() {
    if [[ -n "$SKILLS_SOURCE" ]]; then
        if [[ -d "$SKILLS_SOURCE" ]]; then
            echo "$SKILLS_SOURCE"
            return 0
        fi
        log_error "Skills source not found: $SKILLS_SOURCE"
        return 1
    fi

    for src in "${DEFAULT_SKILLS_SOURCES[@]}"; do
        if [[ -d "$src" ]]; then
            echo "$src"
            return 0
        fi
    done

    log_error "No skills source directory found."
    log_error "Checked: ${DEFAULT_SKILLS_SOURCES[*]}"
    log_error "Use --source <path> to specify one."
    return 1
}

# List skills in a directory (directories containing SKILL.md)
list_skills_in_dir() {
    local dir="$1"
    local count=0

    if [[ ! -d "$dir" ]]; then
        return 0
    fi

    for skill_dir in "$dir"/*/; do
        [[ ! -d "$skill_dir" ]] && continue
        local name
        name="$(basename "$skill_dir")"
        if [[ -f "$skill_dir/SKILL.md" ]]; then
            # Extract description from SKILL.md (first line after <description>)
            local desc=""
            if [[ -f "$skill_dir/SKILL.md" ]]; then
                desc="$(grep -m1 'description' "$skill_dir/SKILL.md" 2>/dev/null | head -1 | sed 's/.*description[>:]\s*//' | cut -c1-60)" || true
            fi
            printf "  %-30s %s\n" "$name" "${DIM}${desc}${RESET}"
            ((count++)) || true
        fi
    done

    return 0
}

# --- Actions ------------------------------------------------------------------

do_list() {
    local source_dir
    source_dir="$(find_skills_source)" || exit 1

    log_info "Available skills in ${BOLD}${source_dir}${RESET}"
    echo ""
    list_skills_in_dir "$source_dir"
    echo ""
}

do_list_user() {
    if [[ -z "$TARGET_USER" ]]; then
        log_error "Specify --user <name>"
        exit 1
    fi

    resolve_user "$TARGET_USER" || exit 1

    local skills_dir="$RESOLVED_HOME/.pi/agent/skills"
    log_info "Skills installed for ${BOLD}${RESOLVED_LINUX_USER}${RESET} ($skills_dir)"
    echo ""

    if [[ ! -d "$skills_dir" ]]; then
        log_warn "No skills directory found"
        return 0
    fi

    list_skills_in_dir "$skills_dir"
    echo ""
}

install_skill_for_user() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    local source_dir
    source_dir="$(find_skills_source)" || return 1

    local skill_source="$source_dir/$SKILL_NAME"
    if [[ ! -d "$skill_source" ]]; then
        log_fail "Skill not found in source: $SKILL_NAME"
        return 1
    fi

    if [[ ! -f "$skill_source/SKILL.md" ]]; then
        log_fail "Invalid skill (no SKILL.md): $SKILL_NAME"
        return 1
    fi

    local target="$home/.pi/agent/skills/$SKILL_NAME"

    if $DRY_RUN; then
        log_step "[dry-run] Would install $SKILL_NAME -> $target"
        return 0
    fi

    copy_as_user "$linux_username" "$skill_source" "$target"
    log_step "Installed $SKILL_NAME for $linux_username"
}

install_single_skill() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    log_info "Installing skill ${BOLD}${SKILL_NAME}${RESET} for ${linux_username}"
    install_skill_for_user "$user_id" "$linux_username" "$home"
    log_ok "Skill $SKILL_NAME installed for $linux_username"
}

install_all_skills() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    log_info "Installing all skills for ${BOLD}${linux_username}${RESET}"

    local source_dir
    source_dir="$(find_skills_source)" || return 1

    local count=0
    for skill_dir in "$source_dir"/*/; do
        [[ ! -d "$skill_dir" ]] && continue
        local name
        name="$(basename "$skill_dir")"
        [[ ! -f "$skill_dir/SKILL.md" ]] && continue

        SKILL_NAME="$name"
        install_skill_for_user "$user_id" "$linux_username" "$home" || {
            log_warn "Failed to install $name"
            continue
        }
        ((count++)) || true
    done

    log_ok "Installed $count skills for $linux_username"
}

remove_skill_for_user() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    log_info "Removing skill ${BOLD}${SKILL_NAME}${RESET} from ${linux_username}"

    local target="$home/.pi/agent/skills/$SKILL_NAME"

    if [[ ! -d "$target" ]]; then
        log_warn "Skill not installed: $SKILL_NAME"
        return 0
    fi

    if $DRY_RUN; then
        log_step "[dry-run] Would remove $target"
        return 0
    fi

    if [[ "$(id -un)" == "$linux_username" ]]; then
        rm -rf "$target"
    else
        sudo rm -rf "$target"
    fi

    log_ok "Removed $SKILL_NAME from $linux_username"
}

update_skills_for_user() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    log_info "Updating skills for ${BOLD}${linux_username}${RESET}"

    local source_dir
    source_dir="$(find_skills_source)" || return 1

    local skills_dir="$home/.pi/agent/skills"
    if [[ ! -d "$skills_dir" ]]; then
        log_warn "No skills installed for $linux_username"
        return 0
    fi

    local count=0
    for skill_dir in "$skills_dir"/*/; do
        [[ ! -d "$skill_dir" ]] && continue
        local name
        name="$(basename "$skill_dir")"

        # Only update if source has this skill
        if [[ -d "$source_dir/$name" && -f "$source_dir/$name/SKILL.md" ]]; then
            SKILL_NAME="$name"
            install_skill_for_user "$user_id" "$linux_username" "$home" || {
                log_warn "Failed to update $name"
                continue
            }
            ((count++)) || true
        fi
    done

    log_ok "Updated $count skills for $linux_username"
}

# --- Main dispatch ------------------------------------------------------------

case "${MODE:-}" in
    list)
        do_list
        ;;
    list-user)
        do_list_user
        ;;
    install)
        if [[ -z "$SKILL_NAME" ]]; then
            log_error "Specify skill name: --install <name>"
            exit 1
        fi
        if [[ -z "$TARGET_USER" ]] && ! $TARGET_ALL; then
            log_error "Specify --user <name> or --all"
            exit 1
        fi
        for_each_user install_single_skill ${TARGET_USER:+--user "$TARGET_USER"} ${TARGET_ALL:+--all}
        ;;
    install-all)
        if [[ -z "$TARGET_USER" ]] && ! $TARGET_ALL; then
            log_error "Specify --user <name> or --all"
            exit 1
        fi
        for_each_user install_all_skills ${TARGET_USER:+--user "$TARGET_USER"} ${TARGET_ALL:+--all}
        ;;
    remove)
        if [[ -z "$SKILL_NAME" ]]; then
            log_error "Specify skill name: --remove <name>"
            exit 1
        fi
        if [[ -z "$TARGET_USER" ]] && ! $TARGET_ALL; then
            log_error "Specify --user <name> or --all"
            exit 1
        fi
        for_each_user remove_skill_for_user ${TARGET_USER:+--user "$TARGET_USER"} ${TARGET_ALL:+--all}
        ;;
    update)
        if [[ -z "$TARGET_USER" ]] && ! $TARGET_ALL; then
            log_error "Specify --user <name> or --all"
            exit 1
        fi
        for_each_user update_skills_for_user ${TARGET_USER:+--user "$TARGET_USER"} ${TARGET_ALL:+--all}
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
