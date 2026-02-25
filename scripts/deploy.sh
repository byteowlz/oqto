#!/usr/bin/env bash
# Oqto deployment script - deploys built binaries and frontend to configured hosts.
#
# Usage:
#   ./scripts/deploy.sh [OPTIONS]
#
# Options:
#   --host NAME       Deploy only to this host (can be repeated)
#   --skip-build      Skip building, deploy existing artifacts
#   --skip-frontend   Skip frontend deployment
#   --skip-backend    Skip backend binary deployment
#   --skip-services   Skip service restarts
#   --dry-run         Show what would be done without doing it
#   --config FILE     Use alternate hosts config (default: deploy/hosts.toml)
#   --help            Show this help
#
# Requires: bash 4+, toml parsing via sed/awk (no external deps)

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="${ROOT_DIR}/deploy/hosts.toml"
DRY_RUN=false
SKIP_BUILD=false
SKIP_FRONTEND=false
SKIP_BACKEND=false
SKIP_SERVICES=false
declare -a HOST_FILTER=()

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

log()  { echo -e "${BLUE}[deploy]${NC} $*"; }
ok()   { echo -e "${GREEN}[deploy]${NC} $*"; }
warn() { echo -e "${YELLOW}[deploy]${NC} $*"; }
err()  { echo -e "${RED}[deploy]${NC} $*" >&2; }
run()  {
    if $DRY_RUN; then
        echo -e "${YELLOW}  [dry-run]${NC} $*"
    else
        "$@"
    fi
}

usage() {
    sed -n '/^# Usage:/,/^[^#]/p' "$0" | grep '^#' | sed 's/^# \?//'
    exit 0
}

# --- Parse arguments ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --host)      HOST_FILTER+=("$2"); shift 2 ;;
        --skip-build)    SKIP_BUILD=true; shift ;;
        --skip-frontend) SKIP_FRONTEND=true; shift ;;
        --skip-backend)  SKIP_BACKEND=true; shift ;;
        --skip-services) SKIP_SERVICES=true; shift ;;
        --dry-run)       DRY_RUN=true; shift ;;
        --config)        CONFIG="$2"; shift 2 ;;
        --help|-h)       usage ;;
        *) err "Unknown option: $1"; usage ;;
    esac
done

if [[ ! -f "$CONFIG" ]]; then
    err "Config not found: $CONFIG"
    exit 1
fi

# --- Parse hosts.toml ---
# Simple TOML parser for our specific format. Reads [[host]] blocks
# into parallel arrays. Handles strings, booleans, and arrays.

declare -a H_NAME=() H_SSH=() H_MODE=() H_USER=() H_FRONTEND=() H_WEB_ROOT=()
declare -a H_BINARIES=() H_SERVICES=() H_LOCAL=()

parse_hosts() {
    local idx=-1
    local in_host=false

    while IFS= read -r line; do
        # Strip comments and leading/trailing whitespace
        line="${line%%#*}"
        line="$(echo "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
        [[ -z "$line" ]] && continue

        # New host block
        if [[ "$line" == "[[host]]" ]]; then
            idx=$((idx + 1))
            in_host=true
            H_NAME[$idx]=""
            H_SSH[$idx]=""
            H_MODE[$idx]="local"
            H_USER[$idx]=""
            H_FRONTEND[$idx]="false"
            H_WEB_ROOT[$idx]=""
            H_BINARIES[$idx]=""
            H_SERVICES[$idx]=""
            H_LOCAL[$idx]="false"
            continue
        fi

        if ! $in_host; then continue; fi

        # Parse key = value
        local key val
        key="$(echo "$line" | sed 's/[[:space:]]*=.*//')"
        val="$(echo "$line" | sed 's/[^=]*=[[:space:]]*//')"

        # Remove surrounding quotes from strings
        val="$(echo "$val" | sed 's/^"//;s/"$//')"

        case "$key" in
            name)      H_NAME[$idx]="$val" ;;
            ssh)       H_SSH[$idx]="$val" ;;
            mode)      H_MODE[$idx]="$val" ;;
            user)      H_USER[$idx]="$val" ;;
            frontend)  H_FRONTEND[$idx]="$val" ;;
            web_root)  H_WEB_ROOT[$idx]="$val" ;;
            local)     H_LOCAL[$idx]="$val" ;;
            binaries)
                # Parse TOML array: ["a", "b", "c"] -> space-separated
                val="$(echo "$val" | tr -d '[]"' | tr ',' ' ')"
                H_BINARIES[$idx]="$val"
                ;;
            services)
                val="$(echo "$val" | tr -d '[]"' | tr ',' ' ')"
                H_SERVICES[$idx]="$val"
                ;;
        esac
    done < "$CONFIG"
}

parse_hosts
HOST_COUNT="${#H_NAME[@]}"

if [[ "$HOST_COUNT" -eq 0 ]]; then
    err "No hosts found in $CONFIG"
    exit 1
fi

# --- Filter hosts ---
should_deploy() {
    local name="$1"
    if [[ ${#HOST_FILTER[@]} -eq 0 ]]; then
        return 0  # No filter = deploy all
    fi
    for f in "${HOST_FILTER[@]}"; do
        if [[ "$f" == "$name" ]]; then
            return 0
        fi
    done
    return 1
}

# --- Build phase ---
if ! $SKIP_BUILD; then
    log "Building backend and frontend..."

    if ! $SKIP_BACKEND; then
        log "Building backend binaries (remote-build)..."
        run bash -c "cd '$ROOT_DIR/backend' && remote-build build --release -p oqto --bin oqto --bin oqto-runner --bin oqto-sandbox"
        run bash -c "cd '$ROOT_DIR/backend' && remote-build build --release -p oqto-files --bin oqto-files"
        run bash -c "cd '$ROOT_DIR/backend' && remote-build build --release -p oqto-usermgr --bin oqto-usermgr"
    fi

    if ! $SKIP_FRONTEND; then
        log "Building frontend..."
        run bash -c "cd '$ROOT_DIR/frontend' && bun run build"
    fi

    ok "Build complete"
fi

# --- Deploy to a single host ---
deploy_host() {
    local name="$1"
    local ssh_target="$2"
    local mode="$3"
    local user="$4"
    local is_frontend="$5"
    local web_root="$6"
    local binaries="$7"
    local services="$8"
    local is_local="$9"

    echo ""
    log "=========================================="
    log "Deploying to ${BOLD}$name${NC} ($mode mode)"
    log "=========================================="

    # --- Check connectivity ---
    if [[ "$is_local" != "true" ]]; then
        log "Checking SSH connectivity to $ssh_target..."
        if ! ssh -o ConnectTimeout=5 "$ssh_target" "echo ok" &>/dev/null; then
            err "Cannot reach $ssh_target via SSH. Skipping."
            return 1
        fi
        ok "SSH connected to $ssh_target"
    fi

    # --- Deploy backend binaries ---
    if ! $SKIP_BACKEND && [[ -n "$binaries" ]]; then
        log "Deploying binaries: $binaries"
        local bin bin_path tmp
        for bin in $binaries; do
            bin_path="$ROOT_DIR/backend/target/release/$bin"
            if [[ ! -f "$bin_path" ]]; then
                warn "Binary not found: $bin_path (skipping)"
                continue
            fi

            if [[ "$is_local" == "true" ]]; then
                log "  Installing $bin -> /usr/local/bin/$bin"
                run sudo install -m 0755 "$bin_path" "/usr/local/bin/$bin"
            else
                log "  Uploading $bin -> $ssh_target:/usr/local/bin/$bin"
                if $DRY_RUN; then
                    echo -e "${YELLOW}  [dry-run]${NC} scp $bin_path -> /usr/local/bin/$bin"
                else
                    tmp="/tmp/oqto-deploy-${bin}"
                    scp -q "$bin_path" "${ssh_target}:${tmp}"
                    ssh "$ssh_target" "sudo install -m 0755 '$tmp' '/usr/local/bin/$bin' && rm -f '$tmp'"
                fi
            fi
        done
        ok "Binaries deployed"
    fi

    # --- Deploy frontend ---
    if ! $SKIP_FRONTEND && [[ "$is_frontend" == "true" ]] && [[ -n "$web_root" ]]; then
        local dist_dir="$ROOT_DIR/frontend/dist"
        if [[ ! -d "$dist_dir" ]]; then
            warn "Frontend dist not found at $dist_dir (skipping)"
        else
            log "Deploying frontend to $web_root"
            if [[ "$is_local" == "true" ]]; then
                run sudo rm -rf "${web_root}/assets"
                run sudo cp -a "${dist_dir}/." "${web_root}/"
            else
                if $DRY_RUN; then
                    echo -e "${YELLOW}  [dry-run]${NC} rsync frontend/dist/ -> $ssh_target:$web_root/"
                else
                    local tmp_dir="/tmp/oqto-deploy-frontend"
                    ssh "$ssh_target" "rm -rf $tmp_dir && mkdir -p $tmp_dir"
                    rsync -az --delete "${dist_dir}/" "${ssh_target}:${tmp_dir}/"
                    ssh "$ssh_target" "sudo rm -rf '${web_root}/assets' && sudo cp -a '${tmp_dir}/.' '${web_root}/' && sudo chown -R ${user}:${user} '${web_root}' && rm -rf '$tmp_dir'"
                fi
            fi
            ok "Frontend deployed"
        fi
    fi

    # --- Restart services ---
    if ! $SKIP_SERVICES && [[ -n "$services" ]]; then
        log "Restarting services: $services"
        local svc
        for svc in $services; do
            log "  Restarting $svc..."
            if [[ "$is_local" == "true" ]]; then
                run systemctl --user restart "$svc" || warn "  Failed to restart $svc"
            else
                if $DRY_RUN; then
                    echo -e "${YELLOW}  [dry-run]${NC} ssh $ssh_target systemctl --user restart $svc"
                else
                    ssh "$ssh_target" "systemctl --user restart $svc" || warn "  Failed to restart $svc on $name"
                fi
            fi
        done

        # Multi-user mode: restart all per-user runners
        if [[ "$mode" == "multi-user" ]]; then
            log "  Restarting per-user runners (multi-user mode)..."
            if $DRY_RUN; then
                echo -e "${YELLOW}  [dry-run]${NC} ssh $ssh_target: restart all per-user oqto-runner processes"
            else
                local runner_pids
                runner_pids="$(ssh "$ssh_target" "pgrep -f 'oqto-runner --socket' 2>/dev/null" || true)"
                if [[ -n "$runner_pids" ]]; then
                    ssh "$ssh_target" "sudo pkill -f 'oqto-runner --socket' || true"
                    log "  Killed running per-user runners. They will respawn on next session."
                else
                    log "  No per-user runners running."
                fi

                local oqto_users oqto_user uid
                oqto_users="$(ssh "$ssh_target" "getent passwd | grep '^oqto_' | cut -d: -f1 | head -50" || true)"
                if [[ -n "$oqto_users" ]]; then
                    log "  Restarting per-user services for platform users..."
                    for oqto_user in $oqto_users; do
                        uid="$(ssh "$ssh_target" "id -u '$oqto_user' 2>/dev/null" || true)"
                        if [[ -n "$uid" ]]; then
                            ssh "$ssh_target" "sudo systemctl restart user@${uid}.service 2>/dev/null" || true
                        fi
                    done
                fi
            fi
        fi

        ok "Services restarted"
    fi

    ok "Deployment to ${BOLD}$name${NC} complete"
}

# --- Deploy to each host ---
for ((i=0; i<HOST_COUNT; i++)); do
    if ! should_deploy "${H_NAME[$i]}"; then
        log "Skipping ${BOLD}${H_NAME[$i]}${NC} (filtered out)"
        continue
    fi

    deploy_host \
        "${H_NAME[$i]}" \
        "${H_SSH[$i]}" \
        "${H_MODE[$i]}" \
        "${H_USER[$i]}" \
        "${H_FRONTEND[$i]}" \
        "${H_WEB_ROOT[$i]}" \
        "${H_BINARIES[$i]}" \
        "${H_SERVICES[$i]}" \
        "${H_LOCAL[$i]}" \
        || warn "Deployment to ${H_NAME[$i]} had errors"
done

echo ""
ok "=========================================="
ok "All deployments complete"
ok "=========================================="
