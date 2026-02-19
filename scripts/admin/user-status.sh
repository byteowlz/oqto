#!/usr/bin/env bash
# user-status.sh - Show detailed provisioning status for all users
#
# Displays a comprehensive overview of each user's provisioning state:
#   - Linux user/UID mapping
#   - EAVS key status
#   - Pi configuration files
#   - Skills installed
#   - Runner socket status
#   - Bootstrap documents
#
# Usage:
#   user-status.sh                  Show status for all users
#   user-status.sh --user alice     Show status for alice only
#   user-status.sh --json           Output as JSON

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

TARGET_USER=""
JSON_OUTPUT=false

usage() {
    cat <<EOF
${BOLD}user-status${RESET} - Show detailed provisioning status for users

${BOLD}USAGE${RESET}
    user-status.sh [options]

${BOLD}OPTIONS${RESET}
    --user, -u <name>   Show status for a specific user only
    --json              Output as JSON
    -h, --help          Show this help

EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user|-u)  TARGET_USER="$2"; shift 2 ;;
        --json)     JSON_OUTPUT=true; shift ;;
        -h|--help)  usage ;;
        *)          log_error "Unknown option: $1"; usage ;;
    esac
done

check_file() {
    local path="$1"
    if [[ -f "$path" ]]; then
        echo "OK"
    else
        echo "MISSING"
    fi
}

check_dir_count() {
    local path="$1"
    if [[ -d "$path" ]]; then
        find "$path" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | wc -l
    else
        echo "0"
    fi
}

check_socket() {
    local path="$1"
    if [[ -S "$path" ]]; then
        # Try to connect
        if timeout 1 bash -c "echo > /dev/tcp/localhost/1 2>/dev/null || true" 2>/dev/null; then
            echo "LISTENING"
        fi
        # Just check if socket exists and is connectable
        if python3 -c "
import socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
try:
    s.connect('$path')
    s.close()
    print('LISTENING')
except:
    print('STALE')
" 2>/dev/null; then
            return 0
        fi
        echo "EXISTS"
    else
        echo "MISSING"
    fi
}

show_user_status() {
    local user_id="$1"
    local username="$2"
    local linux_username="$3"
    local linux_uid="$4"
    local role="$5"
    local is_active="$6"

    local home=""
    if [[ -n "$linux_username" ]]; then
        home="$(get_user_home "$linux_username" 2>/dev/null)" || true
    fi

    if $JSON_OUTPUT; then
        local pi_dir="${home:+$home/.pi/agent}"
        local skills_count=0
        [[ -n "$pi_dir" ]] && skills_count="$(check_dir_count "$pi_dir/skills")"

        jq -n \
            --arg uid "$user_id" \
            --arg uname "$username" \
            --arg linux "$linux_username" \
            --arg luid "$linux_uid" \
            --arg role "$role" \
            --argjson active "$is_active" \
            --arg home "$home" \
            --arg models "$(check_file "${pi_dir}/models.json")" \
            --arg settings "$(check_file "${pi_dir}/settings.json")" \
            --arg agents_md "$(check_file "${pi_dir}/AGENTS.md")" \
            --arg eavs_env "$(check_file "${home:+$home}/.config/oqto/eavs.env")" \
            --argjson skills "$skills_count" \
            '{
                user_id: $uid,
                username: $uname,
                linux_username: $linux,
                linux_uid: ($luid | if . == "" then null else tonumber end),
                role: $role,
                active: $active,
                home: $home,
                provisioning: {
                    models_json: $models,
                    settings_json: $settings,
                    agents_md: $agents_md,
                    eavs_env: $eavs_env,
                    skills_count: $skills
                }
            }'
        return 0
    fi

    # Pretty print
    echo ""
    echo -e "${BOLD}User: ${username}${RESET} (id: ${user_id})"
    echo -e "  Role: ${role}  Active: ${is_active}"
    echo -e "  Linux user: ${linux_username:-${YELLOW}(none)${RESET}}  UID: ${linux_uid:-?}"
    echo -e "  Home: ${home:-${YELLOW}(unknown)${RESET}}"

    if [[ -z "$home" ]]; then
        echo -e "  ${YELLOW}Cannot check provisioning (no home directory)${RESET}"
        return 0
    fi

    local pi_dir="$home/.pi/agent"

    # Pi config files
    echo ""
    echo -e "  ${BOLD}Pi Configuration:${RESET}"
    for item in \
        "models.json:$pi_dir/models.json" \
        "settings.json:$pi_dir/settings.json" \
        "AGENTS.md:$pi_dir/AGENTS.md" \
        "eavs.env:$home/.config/oqto/eavs.env"; do

        local name="${item%%:*}"
        local path="${item##*:}"
        local status
        status="$(check_file "$path")"

        if [[ "$status" == "OK" ]]; then
            local size
            size="$(wc -c < "$path" 2>/dev/null || echo "?")"
            printf "    ${GREEN}%-20s${RESET} %s (%s bytes)\n" "$name" "$status" "$size"
        else
            printf "    ${YELLOW}%-20s${RESET} %s\n" "$name" "$status"
        fi
    done

    # Skills
    local skills_dir="$pi_dir/skills"
    local skills_count
    skills_count="$(check_dir_count "$skills_dir")"
    echo ""
    echo -e "  ${BOLD}Skills:${RESET} ${skills_count} installed"
    if [[ "$skills_count" -gt 0 && "$skills_count" -le 10 ]]; then
        for skill in "$skills_dir"/*/; do
            [[ ! -d "$skill" ]] && continue
            printf "    %s\n" "$(basename "$skill")"
        done
    elif [[ "$skills_count" -gt 10 ]]; then
        # Just show first 5 and count
        local shown=0
        for skill in "$skills_dir"/*/; do
            [[ ! -d "$skill" ]] && continue
            printf "    %s\n" "$(basename "$skill")"
            ((shown++)) || true
            [[ $shown -ge 5 ]] && break
        done
        echo "    ... and $((skills_count - 5)) more"
    fi

    # Bootstrap docs
    echo ""
    echo -e "  ${BOLD}Bootstrap Documents:${RESET}"
    for doc in ONBOARD.md PERSONALITY.md USER.md; do
        local status
        status="$(check_file "$pi_dir/$doc")"
        if [[ "$status" == "OK" ]]; then
            printf "    ${GREEN}%-20s${RESET} %s\n" "$doc" "$status"
        else
            printf "    ${DIM}%-20s${RESET} %s\n" "$doc" "$status"
        fi
    done

    # Runner socket
    echo ""
    echo -e "  ${BOLD}Runner:${RESET}"
    local socket_path="/run/oqto/runner-sockets/${linux_username}/oqto-runner.sock"
    if [[ -S "$socket_path" ]]; then
        printf "    ${GREEN}%-20s${RESET} %s\n" "Socket" "$socket_path"
    else
        printf "    ${YELLOW}%-20s${RESET} %s\n" "Socket" "MISSING ($socket_path)"
    fi
}

# --- Main ---------------------------------------------------------------------

db="$(find_db)" || exit 1

# Build query
query="SELECT id, username, linux_username, COALESCE(linux_uid, ''), role, is_active FROM users"
if [[ -n "$TARGET_USER" ]]; then
    query="$query WHERE id = '$TARGET_USER' OR username = '$TARGET_USER'"
else
    query="$query WHERE is_active = 1"
fi
query="$query ORDER BY username"

if $JSON_OUTPUT; then
    # Collect all user JSON objects into an array
    results=()
    while IFS='|' read -r user_id username linux_username linux_uid role is_active; do
        result="$(show_user_status "$user_id" "$username" "$linux_username" "$linux_uid" "$role" "$is_active")"
        results+=("$result")
    done < <(sqlite3 -separator '|' "$db" "$query")

    # Combine into JSON array
    printf '%s\n' "${results[@]}" | jq -s '.'
else
    echo ""
    echo -e "${BOLD}=== Oqto User Provisioning Status ===${RESET}"

    count=0
    while IFS='|' read -r user_id username linux_username linux_uid role is_active; do
        show_user_status "$user_id" "$username" "$linux_username" "$linux_uid" "$role" "$is_active"
        ((count++)) || true
    done < <(sqlite3 -separator '|' "$db" "$query")

    echo ""
    echo -e "${DIM}Total: $count user(s)${RESET}"
    echo ""
fi
