#!/usr/bin/env bash
# Sourced by deploy.sh. No policy changes, password storage, or late prompts.
# shellcheck disable=SC2034  # main driver uses these globals through functions
SUDO_REFRESH_SECONDS=30
SUDO_LEASE_DIR=""
declare -a SUDO_REFRESH_PIDS=()
declare -A SUDO_REMOTE_CHANNELS=()

sudo_refresh_host() (
    # Only local tickets are refreshed. Remote hosts retain one sudo process;
    # an unrelated SSH session cannot refresh its tty/PPID-scoped ticket.
    exec sudo -n -v
)

start_remote_privilege_channel() {
    local i="$1" path="$SUDO_LEASE_DIR/remote-$1.sock" ready="$SUDO_LEASE_DIR/remote-$1.ready" pid option
    local -a options=()
    for option in "${OQTO_SSH_OPTIONS[@]}"; do options+=("--ssh-option=$option"); done
    python3 "$ROOT_DIR/scripts/dist/deploy-privilege-channel.py" serve \
        --host "${H_SSH[$i]}" --socket "$path" --ready "$ready" "${options[@]}" &
    pid=$!
    SUDO_REFRESH_PIDS+=("$pid")
    while [[ ! -f "$ready" ]]; do
        if ! kill -0 "$pid" 2>/dev/null; then
            wait "$pid" 2>/dev/null || true
            err "[${H_NAME[$i]}] remote sudo session failed before build"
            return 1
        fi
        sleep 0.1
    done
    SUDO_REMOTE_CHANNELS["${H_SSH[$i]}"]="$path"
}

remote_privileged_exec() {
    local target="$1" script="$2" path="${SUDO_REMOTE_CHANNELS[$1]:-}"
    [[ -n "$path" ]] || { err "No authenticated privilege channel for $target"; return 1; }
    printf '%s' "$script" | python3 "$ROOT_DIR/scripts/dist/deploy-privilege-channel.py" run --socket "$path"
}

require_sudo_leases() {
    [[ "$DRY_RUN" == true ]] && return 0
    if [[ -z "$SUDO_LEASE_DIR" || -e "$SUDO_LEASE_DIR/failed" ]]; then
        err 'Deployment sudo authorization was lost; stopping without another password prompt. Rerun the deployment.'
        return 1
    fi
    local pid
    for pid in "${SUDO_REFRESH_PIDS[@]}"; do
        if ! kill -0 "$pid" 2>/dev/null; then
            err 'Deployment sudo refresh worker stopped; refusing further privileged actions.'
            return 1
        fi
    done
}

stop_sudo_leases() {
    local pid
    for pid in "${SUDO_REFRESH_PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${SUDO_REFRESH_PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
    SUDO_REFRESH_PIDS=()
    SUDO_REMOTE_CHANNELS=()
    if [[ -n "$SUDO_LEASE_DIR" ]]; then rm -rf -- "$SUDO_LEASE_DIR"; SUDO_LEASE_DIR=""; fi
}

sudo_refresh_loop() {
    local i="$1" sleeper="" refresher="" child=""
    # Reap both the timer and in-flight SSH/sudo process on interruption.
    trap 'for child in "$sleeper" "$refresher"; do if [[ -n "$child" ]]; then kill "$child" 2>/dev/null || true; wait "$child" 2>/dev/null || true; fi; done' EXIT
    trap 'exit 0' TERM INT
    while true; do
        sleep "$SUDO_REFRESH_SECONDS" & sleeper=$!
        wait "$sleeper" || return 0
        sleeper=""
        sudo_refresh_host "$i" </dev/null & refresher=$!
        if ! wait "$refresher"; then
            printf '%s\n' "${H_NAME[$i]}" > "$SUDO_LEASE_DIR/failed"
            err "[${H_NAME[$i]}] sudo refresh failed; no further privileged actions will run."
            return 1
        fi
        refresher=""
    done
}

start_sudo_leases() {
    [[ "$DRY_RUN" == true ]] && return 0
    local i
    SUDO_LEASE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oqto-deploy-auth.XXXXXXXX")" || return 1
    for i in "$@"; do
        log "[${H_NAME[$i]}] authorizing deployment before build"
        if [[ "${H_LOCAL[$i]}" != true ]]; then
            start_remote_privilege_channel "$i" || return 1
            remote_privileged_exec "${H_SSH[$i]}" true || return 1
            continue
        fi
        sudo -v || return 1
        # Probe both local command and refresh scopes before the long build.
        host_exec "${H_LOCAL[$i]}" "${H_SSH[$i]}" 'sudo -n true' || {
            err "[${H_NAME[$i]}] sudo authorization does not cover noninteractive deployment commands; refusing before build. Check the host sudo timestamp policy."
            return 1
        }
        # A refresh worker must also be able to use this authorization scope.
        (sudo_refresh_host "$i" </dev/null) || {
            err "[${H_NAME[$i]}] sudo authorization cannot be refreshed noninteractively; refusing before build."
            return 1
        }
        sudo_refresh_loop "$i" </dev/null &
        SUDO_REFRESH_PIDS+=("$!")
    done
    require_sudo_leases
}
