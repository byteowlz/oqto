#!/usr/bin/env bash
# eavs-provision.sh - Provision or re-provision EAVS API keys for users
#
# This handles cases where automatic EAVS provisioning failed during user
# creation, or when keys need to be rotated/regenerated.
#
# Operations:
#   1. Creates a new EAVS virtual key for the user (or rotates existing)
#   2. Writes eavs.env with the key + EAVS URL
#   3. Regenerates models.json from the current EAVS provider catalog
#
# Usage:
#   eavs-provision.sh --user <username>    Provision for a single user
#   eavs-provision.sh --all                Provision for all active users
#   eavs-provision.sh --sync-models        Only regenerate models.json (no key changes)
#   eavs-provision.sh --rotate --user <u>  Revoke old key and create new one
#   eavs-provision.sh --status             Show EAVS key status for all users

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

MODE="provision"    # provision | sync-models | rotate | status
TARGET_USER=""
TARGET_ALL=false
DRY_RUN=false

usage() {
    cat <<EOF
${BOLD}eavs-provision${RESET} - Provision EAVS API keys for Oqto users

${BOLD}USAGE${RESET}
    eavs-provision.sh [options]

${BOLD}OPTIONS${RESET}
    --user, -u <name>   Target a specific user (username or user ID)
    --all, -a           Target all active users
    --sync-models       Only regenerate models.json (no key creation/rotation)
    --rotate            Revoke existing key and create a new one
    --status            Show EAVS key status for all users
    --dry-run           Show what would be done without making changes
    -h, --help          Show this help

${BOLD}EXAMPLES${RESET}
    eavs-provision.sh --user alice          # Provision EAVS for alice
    eavs-provision.sh --all                 # Provision EAVS for all users
    eavs-provision.sh --sync-models --all   # Regenerate models.json for all
    eavs-provision.sh --rotate --user bob   # Rotate bob's EAVS key
    eavs-provision.sh --status              # Show key status

EOF
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --user|-u)    TARGET_USER="$2"; shift 2 ;;
        --all|-a)     TARGET_ALL=true; shift ;;
        --sync-models) MODE="sync-models"; shift ;;
        --rotate)     MODE="rotate"; shift ;;
        --status)     MODE="status"; shift ;;
        --dry-run)    DRY_RUN=true; shift ;;
        -h|--help)    usage ;;
        *)            log_error "Unknown option: $1"; usage ;;
    esac
done

# Get EAVS connection details
EAVS_BASE_URL="$(get_eavs_url)"

# --- EAVS API helpers --------------------------------------------------------

eavs_api() {
    local method="$1"
    local path="$2"
    local data="${3:-}"

    local master_key
    master_key="$(get_eavs_master_key)" || exit 1

    local url="${EAVS_BASE_URL}${path}"
    local args=(
        -s -f
        -X "$method"
        -H "Authorization: Bearer $master_key"
        -H "Content-Type: application/json"
    )

    if [[ -n "$data" ]]; then
        args+=(-d "$data")
    fi

    curl "${args[@]}" "$url" 2>/dev/null
}

# Create a virtual key for a user
create_eavs_key() {
    local user_id="$1"
    local key_name="oqto-user-${user_id}"

    local payload
    payload=$(jq -n --arg name "$key_name" --arg user "$user_id" '{
        name: $name,
        oauth_user: $user
    }')

    eavs_api POST "/admin/keys" "$payload"
}

# List all EAVS keys
list_eavs_keys() {
    eavs_api GET "/admin/keys"
}

# Revoke an EAVS key by ID
revoke_eavs_key() {
    local key_id="$1"
    eavs_api DELETE "/admin/keys/$key_id"
}

# Get provider details for models.json generation
get_provider_details() {
    curl -sf "${EAVS_BASE_URL}/providers/detail" 2>/dev/null
}

# Generate models.json from provider details.
# $1: providers JSON from eavs /providers/detail
# $2: eavs base URL
# $3: (optional) API key to embed. Defaults to "not-needed".
generate_models_json() {
    local providers_json="$1"
    local eavs_base="$2"
    local api_key="${3:-not-needed}"

    echo "$providers_json" | jq --arg base "$eavs_base" --arg key "$api_key" '
    {
        providers: (
            [.[] | select(.name != "default" and .pi_api != null)] |
            map({
                key: ("eavs-" + .name),
                value: {
                    baseUrl: ($base + "/" + .name + "/v1"),
                    api: .pi_api,
                    apiKey: $key,
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
    }'
}

# --- Per-user operations ------------------------------------------------------

provision_user() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    log_info "Provisioning EAVS for ${BOLD}${linux_username}${RESET} (user_id: $user_id)"

    if $DRY_RUN; then
        log_step "[dry-run] Would create EAVS key for $user_id"
        log_step "[dry-run] Would write $home/.config/oqto/eavs.env"
        log_step "[dry-run] Would write $home/.pi/agent/models.json"
        return 0
    fi

    # 1. Create EAVS key
    local key_response
    key_response="$(create_eavs_key "$user_id")" || {
        log_fail "Failed to create EAVS key for $user_id"
        return 1
    }

    local api_key key_id
    api_key="$(echo "$key_response" | jq -r '.key')"
    key_id="$(echo "$key_response" | jq -r '.key_id // .id // "unknown"')"

    if [[ -z "$api_key" || "$api_key" == "null" ]]; then
        log_fail "EAVS returned empty key for $user_id"
        return 1
    fi

    log_step "Created EAVS key: $key_id"

    # 2. Generate and write models.json with the key embedded directly.
    # The apiKey field uses the literal virtual key (not an env var reference).
    sync_models_for_user_with_key "$user_id" "$linux_username" "$home" "$api_key"

    log_ok "EAVS provisioned for $linux_username"
}

rotate_user_key() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    log_info "Rotating EAVS key for ${BOLD}${linux_username}${RESET}"

    # Find existing key(s) for this user
    local all_keys
    all_keys="$(list_eavs_keys)" || {
        log_fail "Could not list EAVS keys"
        return 1
    }

    local old_keys
    old_keys="$(echo "$all_keys" | jq -r --arg user "$user_id" \
        '[.[] | select(.name == "oqto-user-" + $user or .oauth_user == $user)] | .[].id // empty')"

    if $DRY_RUN; then
        if [[ -n "$old_keys" ]]; then
            log_step "[dry-run] Would revoke keys: $(echo "$old_keys" | tr '\n' ', ')"
        fi
        log_step "[dry-run] Would create new key and update eavs.env"
        return 0
    fi

    # Revoke old keys
    if [[ -n "$old_keys" ]]; then
        while IFS= read -r key_id; do
            [[ -z "$key_id" ]] && continue
            log_step "Revoking old key: $key_id"
            revoke_eavs_key "$key_id" || log_warn "Failed to revoke key $key_id (may already be revoked)"
        done <<< "$old_keys"
    fi

    # Create new key
    provision_user "$user_id" "$linux_username" "$home"
}

sync_models_for_user_with_key() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"
    local api_key="$4"

    local providers
    providers="$(get_provider_details)" || {
        log_fail "Could not fetch EAVS provider details"
        return 1
    }

    local models_json
    models_json="$(generate_models_json "$providers" "$EAVS_BASE_URL" "$api_key")"

    if $DRY_RUN; then
        log_step "[dry-run] Would write $home/.pi/agent/models.json"
        return 0
    fi

    write_file_as_user "$linux_username" "$home/.pi/agent" "models.json" "$models_json" "644"
    log_step "Updated models.json"
}

sync_models_for_user() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    # Read existing API key from models.json (preserved across regenerations).
    # Falls back to legacy eavs.env, then "not-needed".
    local api_key="not-needed"
    local models_file="$home/.pi/agent/models.json"
    if [[ -f "$models_file" ]]; then
        local existing_key
        existing_key="$(jq -r '[.providers // {} | to_entries[] | .value.apiKey // empty | select(. != "EAVS_API_KEY" and . != "not-needed" and (startswith("env:") | not))] | first // empty' "$models_file" 2>/dev/null)"
        if [[ -n "$existing_key" ]]; then
            api_key="$existing_key"
        fi
    fi
    # Legacy fallback: read from eavs.env
    if [[ "$api_key" == "not-needed" ]]; then
        local eavs_env="$home/.config/oqto/eavs.env"
        if [[ -f "$eavs_env" ]]; then
            local env_key
            env_key="$(grep '^EAVS_API_KEY=' "$eavs_env" 2>/dev/null | head -1 | cut -d= -f2-)"
            if [[ -n "$env_key" ]]; then
                api_key="$env_key"
            fi
        fi
    fi

    sync_models_for_user_with_key "$user_id" "$linux_username" "$home" "$api_key"
}

sync_models_only() {
    local user_id="$1"
    local linux_username="$2"
    local home="$3"

    log_info "Syncing models.json for ${BOLD}${linux_username}${RESET}"
    sync_models_for_user "$user_id" "$linux_username" "$home"
    log_ok "models.json synced for $linux_username"
}

show_status() {
    log_info "EAVS key status for all users"
    echo ""

    local all_keys
    all_keys="$(list_eavs_keys 2>/dev/null)" || {
        log_fail "Could not connect to EAVS at $EAVS_BASE_URL"
        return 1
    }

    printf "${BOLD}%-20s %-20s %-12s %-20s %-8s${RESET}\n" \
        "USERNAME" "LINUX_USER" "KEY_STATUS" "KEY_NAME" "MODELS"

    local db
    db="$(find_db)" || return 1

    while IFS='|' read -r user_id username linux_username; do
        [[ -z "$linux_username" ]] && linux_username="(none)"

        # Check if user has an EAVS key
        local key_info
        key_info="$(echo "$all_keys" | jq -r --arg user "$user_id" \
            '[.[] | select(.name == "oqto-user-" + $user or .oauth_user == $user)] | first // empty')"

        local key_status="MISSING"
        local key_name="-"

        if [[ -n "$key_info" ]]; then
            local enabled
            enabled="$(echo "$key_info" | jq -r '.enabled // true')"
            key_name="$(echo "$key_info" | jq -r '.name // "-"')"
            if [[ "$enabled" == "true" ]]; then
                key_status="${GREEN}ACTIVE${RESET}"
            else
                key_status="${RED}REVOKED${RESET}"
            fi
        else
            key_status="${YELLOW}MISSING${RESET}"
        fi

        # Check if models.json exists
        local models_status="-"
        if [[ -n "$linux_username" && "$linux_username" != "(none)" ]]; then
            local home
            home="$(get_user_home "$linux_username" 2>/dev/null)" || true
            if [[ -n "$home" && -f "$home/.pi/agent/models.json" ]]; then
                models_status="${GREEN}OK${RESET}"
            else
                models_status="${YELLOW}MISSING${RESET}"
            fi
        fi

        printf "%-20s %-20s %-12b %-20s %-8b\n" \
            "$username" "$linux_username" "$key_status" "$key_name" "$models_status"

    done < <(sqlite3 -separator '|' "$db" \
        "SELECT id, username, linux_username FROM users WHERE is_active = 1 ORDER BY username")

    echo ""
}

# --- Main dispatch ------------------------------------------------------------

case "$MODE" in
    provision)
        if [[ -z "$TARGET_USER" ]] && ! $TARGET_ALL; then
            log_error "Specify --user <name> or --all"
            exit 1
        fi
        for_each_user provision_user ${TARGET_USER:+--user "$TARGET_USER"} ${TARGET_ALL:+--all}
        ;;
    sync-models)
        if [[ -z "$TARGET_USER" ]] && ! $TARGET_ALL; then
            log_error "Specify --user <name> or --all"
            exit 1
        fi
        for_each_user sync_models_only ${TARGET_USER:+--user "$TARGET_USER"} ${TARGET_ALL:+--all}
        ;;
    rotate)
        if [[ -z "$TARGET_USER" ]] && ! $TARGET_ALL; then
            log_error "Specify --user <name> or --all for key rotation"
            exit 1
        fi
        for_each_user rotate_user_key ${TARGET_USER:+--user "$TARGET_USER"} ${TARGET_ALL:+--all}
        ;;
    status)
        show_status
        ;;
esac
