#!/usr/bin/env bash
# Sourced by deploy.sh. No policy changes, password storage, or late prompts.
# shellcheck disable=SC2034  # main driver uses these globals through functions
SUDO_REFRESH_SECONDS=30
SUDO_LEASE_DIR=""
declare -a SUDO_REFRESH_PIDS=()

sudo_refresh_host() (
    local i="$1"
    if [[ "${H_LOCAL[$i]}" == true ]]; then
        exec sudo -n -v
    else
        # Use the driver's SSH options (the refresh process must be reaped).
        exec ssh "${OQTO_SSH_OPTIONS[@]}" -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=10 -o ServerAliveCountMax=2 \
            "${H_SSH[$i]}" 'sudo -n -v'
    fi
)

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
        if [[ "${H_LOCAL[$i]}" == true ]]; then
            sudo -v || return 1
        elif ! sudo_refresh_host "$i" </dev/null >/dev/null 2>&1; then
            ssh -tt "${H_SSH[$i]}" 'sudo -v' </dev/tty || return 1
        fi
        # Probe the actual noninteractive execution lane, not merely the tty
        # used for authentication. Tty/PPID-only remote tickets may not apply.
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
