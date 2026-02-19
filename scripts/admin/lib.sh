#!/usr/bin/env bash
# Shared library for oqto-admin scripts
# Source this at the top of each script:
#   source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

set -euo pipefail

# --- Configuration -----------------------------------------------------------

OQTO_SERVER_URL="${OQTO_SERVER_URL:-http://localhost:8080/api}"
OQTO_ADMIN_SOCKET="${OQTO_ADMIN_SOCKET:-/run/oqto/oqtoctl.sock}"
OQTO_CONFIG="${OQTO_CONFIG:-}"
OQTO_DB="${OQTO_DB:-}"

# --- Colors ------------------------------------------------------------------

if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BLUE='\033[0;34m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    DIM='\033[2m'
    RESET='\033[0m'
else
    RED='' GREEN='' YELLOW='' BLUE='' CYAN='' BOLD='' DIM='' RESET=''
fi

log_info()  { echo -e "${GREEN}[INFO]${RESET} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${RESET} $*" >&2; }
log_error() { echo -e "${RED}[ERROR]${RESET} $*" >&2; }
log_step()  { echo -e "${BLUE}  -->  ${RESET}$*"; }
log_ok()    { echo -e "${GREEN}  [OK]${RESET} $*"; }
log_fail()  { echo -e "${RED}[FAIL]${RESET} $*"; }

# --- Config Resolution -------------------------------------------------------

# Find the oqto config file
find_config() {
    if [[ -n "$OQTO_CONFIG" && -f "$OQTO_CONFIG" ]]; then
        echo "$OQTO_CONFIG"
        return 0
    fi

    local xdg_config="${XDG_CONFIG_HOME:-$HOME/.config}"
    local candidates=(
        "$xdg_config/oqto/config.toml"
        "$HOME/.config/oqto/config.toml"
        "/etc/oqto/config.toml"
    )

    for candidate in "${candidates[@]}"; do
        if [[ -f "$candidate" ]]; then
            echo "$candidate"
            return 0
        fi
    done

    log_warn "No oqto config file found"
    return 1
}

# Read a TOML value (simple key = "value" parser, not a full TOML parser)
# Usage: read_toml_value <file> <section> <key>
# Example: read_toml_value config.toml eavs base_url
read_toml_value() {
    local file="$1"
    local section="$2"
    local key="$3"

    if [[ ! -f "$file" ]]; then
        return 1
    fi

    local in_section=false
    while IFS= read -r line; do
        # Skip comments and empty lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "${line// /}" ]] && continue

        # Check for section header
        if [[ "$line" =~ ^\[([^]]+)\] ]]; then
            local found_section="${BASH_REMATCH[1]}"
            if [[ "$found_section" == "$section" ]]; then
                in_section=true
            else
                in_section=false
            fi
            continue
        fi

        # Extract key = "value" within section
        if $in_section && [[ "$line" =~ ^[[:space:]]*${key}[[:space:]]*=[[:space:]]*\"([^\"]*)\" ]]; then
            echo "${BASH_REMATCH[1]}"
            return 0
        fi
        # Also handle key = value (unquoted, e.g. booleans/numbers)
        if $in_section && [[ "$line" =~ ^[[:space:]]*${key}[[:space:]]*=[[:space:]]*([^[:space:]#]+) ]]; then
            echo "${BASH_REMATCH[1]}"
            return 0
        fi
    done < "$file"

    return 1
}

# --- EAVS Configuration ------------------------------------------------------

# Get EAVS base URL from config or env
get_eavs_url() {
    if [[ -n "${EAVS_URL:-}" ]]; then
        echo "$EAVS_URL"
        return 0
    fi

    local config
    config="$(find_config)" || return 1

    # Try [eavs] section first, fall back to [local] eavs keys
    local url
    url="$(read_toml_value "$config" "eavs" "base_url" 2>/dev/null)" ||
    url="$(read_toml_value "$config" "eavs" "url" 2>/dev/null)" || true

    if [[ -n "${url:-}" ]]; then
        echo "$url"
        return 0
    fi

    # Default
    echo "http://localhost:3033"
}

# Get EAVS master key from config or env
get_eavs_master_key() {
    if [[ -n "${EAVS_MASTER_KEY:-}" ]]; then
        echo "$EAVS_MASTER_KEY"
        return 0
    fi

    local config
    config="$(find_config)" || return 1

    local key
    key="$(read_toml_value "$config" "eavs" "master_key" 2>/dev/null)" || true

    if [[ -n "${key:-}" ]]; then
        echo "$key"
        return 0
    fi

    # Try reading from eavs own config
    local eavs_config="${XDG_CONFIG_HOME:-$HOME/.config}/eavs/config.toml"
    if [[ -f "$eavs_config" ]]; then
        key="$(read_toml_value "$eavs_config" "server" "master_key" 2>/dev/null)" || true
        if [[ -n "${key:-}" ]]; then
            echo "$key"
            return 0
        fi
    fi

    log_error "EAVS master key not found. Set EAVS_MASTER_KEY or configure in config.toml"
    return 1
}

# --- Database Access ----------------------------------------------------------

# Find the oqto database
find_db() {
    if [[ -n "$OQTO_DB" ]]; then
        echo "$OQTO_DB"
        return 0
    fi

    local config
    if config="$(find_config)"; then
        local db_path
        db_path="$(read_toml_value "$config" "database" "path" 2>/dev/null)" || true
        if [[ -n "${db_path:-}" && -f "$db_path" ]]; then
            echo "$db_path"
            return 0
        fi
    fi

    # Default locations
    local xdg_data="${XDG_DATA_HOME:-$HOME/.local/share}"
    local candidates=(
        "$xdg_data/oqto/oqto.db"
        "$HOME/.local/share/oqto/oqto.db"
    )

    for candidate in "${candidates[@]}"; do
        if [[ -f "$candidate" ]]; then
            echo "$candidate"
            return 0
        fi
    done

    log_error "Database not found. Set OQTO_DB or check config."
    return 1
}

# --- User Listing -------------------------------------------------------------

# List all platform users as JSON array
# Returns: [{"id":"...", "username":"...", "linux_username":"...", "role":"...", "is_active": true}]
list_users_json() {
    local db
    db="$(find_db)" || return 1

    sqlite3 -json "$db" \
        "SELECT id, username, linux_username, linux_uid, email, role, is_active FROM users ORDER BY username"
}

# List active user IDs (one per line)
list_active_user_ids() {
    local db
    db="$(find_db)" || return 1

    sqlite3 "$db" "SELECT id FROM users WHERE is_active = 1 ORDER BY username"
}

# Get linux username for a user ID
get_linux_username() {
    local user_id="$1"
    local db
    db="$(find_db)" || return 1

    sqlite3 "$db" "SELECT linux_username FROM users WHERE id = '$user_id' OR username = '$user_id' LIMIT 1"
}

# Get user home directory from linux username
get_user_home() {
    local linux_username="$1"

    local home
    home="$(getent passwd "$linux_username" 2>/dev/null | cut -d: -f6)" || true
    if [[ -n "$home" ]]; then
        echo "$home"
        return 0
    fi

    # Fallback: try /home/<username>
    if [[ -d "/home/$linux_username" ]]; then
        echo "/home/$linux_username"
        return 0
    fi

    return 1
}

# Resolve a --user argument to (user_id, linux_username, home)
# Usage: resolve_user "alice"
# Sets: RESOLVED_USER_ID, RESOLVED_LINUX_USER, RESOLVED_HOME
resolve_user() {
    local input="$1"
    local db
    db="$(find_db)" || return 1

    local row
    row="$(sqlite3 -separator '|' "$db" \
        "SELECT id, linux_username FROM users WHERE id = '$input' OR username = '$input' LIMIT 1")"

    if [[ -z "$row" ]]; then
        log_error "User not found: $input"
        return 1
    fi

    RESOLVED_USER_ID="$(echo "$row" | cut -d'|' -f1)"
    RESOLVED_LINUX_USER="$(echo "$row" | cut -d'|' -f2)"

    if [[ -z "$RESOLVED_LINUX_USER" ]]; then
        log_error "User $input has no linux_username assigned"
        return 1
    fi

    RESOLVED_HOME="$(get_user_home "$RESOLVED_LINUX_USER")" || {
        log_error "Could not determine home directory for $RESOLVED_LINUX_USER"
        return 1
    }
}

# Iterate over users and call a function for each
# Usage: for_each_user <function_name> [--all | --user <name>]
# The function receives: user_id linux_username home_dir
for_each_user() {
    local func="$1"
    shift

    local target_all=false
    local target_user=""

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --all|-a)
                target_all=true
                shift
                ;;
            --user|-u)
                target_user="$2"
                shift 2
                ;;
            *)
                log_error "Unknown argument: $1"
                return 1
                ;;
        esac
    done

    if [[ -z "$target_user" ]] && ! $target_all; then
        log_error "Specify --user <name> or --all"
        return 1
    fi

    if [[ -n "$target_user" ]]; then
        resolve_user "$target_user" || return 1
        "$func" "$RESOLVED_USER_ID" "$RESOLVED_LINUX_USER" "$RESOLVED_HOME"
        return $?
    fi

    # All active users
    local db
    db="$(find_db)" || return 1

    local errors=0
    while IFS='|' read -r user_id linux_username; do
        if [[ -z "$linux_username" ]]; then
            log_warn "User $user_id has no linux_username, skipping"
            continue
        fi

        local home
        home="$(get_user_home "$linux_username" 2>/dev/null)" || {
            log_warn "Could not find home for $linux_username, skipping"
            ((errors++)) || true
            continue
        }

        "$func" "$user_id" "$linux_username" "$home" || ((errors++)) || true
    done < <(sqlite3 -separator '|' "$db" \
        "SELECT id, linux_username FROM users WHERE is_active = 1 ORDER BY username")

    if [[ $errors -gt 0 ]]; then
        log_warn "$errors user(s) had errors"
        return 1
    fi
    return 0
}

# --- File Operations as User -------------------------------------------------

# Write a file to a user's home directory with correct ownership
write_file_as_user() {
    local linux_username="$1"
    local target_dir="$2"
    local filename="$3"
    local content="$4"
    local mode="${5:-644}"

    local target_path="$target_dir/$filename"

    # Check if we need sudo
    if [[ "$(id -un)" == "$linux_username" ]]; then
        mkdir -p "$target_dir"
        echo "$content" > "$target_path"
        chmod "$mode" "$target_path"
    else
        local temp_file
        temp_file="$(mktemp)"
        echo "$content" > "$temp_file"

        sudo mkdir -p "$target_dir"
        sudo cp "$temp_file" "$target_path"
        sudo chown "$linux_username:$(id -gn "$linux_username" 2>/dev/null || echo oqto)" "$target_path"
        sudo chmod "$mode" "$target_path"
        rm -f "$temp_file"
    fi
}

# Copy a file/directory to a user's home with correct ownership
copy_as_user() {
    local linux_username="$1"
    local source_path="$2"
    local target_path="$3"

    local group
    group="$(id -gn "$linux_username" 2>/dev/null || echo oqto)"

    if [[ "$(id -un)" == "$linux_username" ]]; then
        if [[ -d "$source_path" ]]; then
            mkdir -p "$target_path"
            cp -a "$source_path/." "$target_path/"
        else
            mkdir -p "$(dirname "$target_path")"
            cp "$source_path" "$target_path"
        fi
    else
        if [[ -d "$source_path" ]]; then
            sudo mkdir -p "$target_path"
            sudo cp -a "$source_path/." "$target_path/"
            sudo chown -R "$linux_username:$group" "$target_path"
        else
            sudo mkdir -p "$(dirname "$target_path")"
            sudo cp "$source_path" "$target_path"
            sudo chown "$linux_username:$group" "$target_path"
        fi
    fi
}

# --- API Helpers --------------------------------------------------------------

# Call oqtoctl with proper auth
oqtoctl_cmd() {
    local args=("$@")

    # Check if oqtoctl is available
    if command -v oqtoctl &>/dev/null; then
        oqtoctl "${args[@]}"
    elif [[ -x /usr/local/bin/oqtoctl ]]; then
        /usr/local/bin/oqtoctl "${args[@]}"
    else
        log_error "oqtoctl not found"
        return 1
    fi
}

# Make a request to the oqto admin API
api_request() {
    local method="$1"
    local path="$2"
    local data="${3:-}"

    local url="${OQTO_SERVER_URL}${path}"
    local auth_args=()

    if [[ -n "${OQTO_AUTH_TOKEN:-}" ]]; then
        auth_args=(-H "Authorization: Bearer $OQTO_AUTH_TOKEN")
    elif [[ -n "${OQTO_DEV_USER:-}" ]]; then
        auth_args=(-H "X-Dev-User: $OQTO_DEV_USER")
    else
        # Try admin socket
        if [[ -S "$OQTO_ADMIN_SOCKET" ]]; then
            url="http://localhost/api${path}"
            auth_args=(--unix-socket "$OQTO_ADMIN_SOCKET")
        fi
    fi

    local curl_args=(
        -s
        -X "$method"
        -H "Content-Type: application/json"
        "${auth_args[@]}"
    )

    if [[ -n "$data" ]]; then
        curl_args+=(-d "$data")
    fi

    curl "${curl_args[@]}" "$url"
}
