#!/usr/bin/env bash
# Oqto deployment script with transactional activation, preflight gates,
# canary rollout, health checks, and automatic rollback.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="${ROOT_DIR}/deploy/hosts.toml"
DRY_RUN=false
SKIP_BUILD=false
SKIP_FRONTEND=false
SKIP_BACKEND=false
SKIP_SERVICES=false
PREPARE_ONLY=false
ACTIVATE_ONLY=false
RESUME=false
STATUS_ONLY=false
CANARY_ONLY=false
CANARY_THEN_FLEET=false
HEALTH_TIMEOUT_SECONDS=90
MIN_FREE_MB=1024
RELEASE_ID=""
EVENT_LOG_PATH="/var/log/oqto/update-events.jsonl"
RELEASES_ROOT="/var/lib/oqto/releases"
DEPENDENCY_POLICY_FILE="$ROOT_DIR/dependencies.toml"

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

usage() {
    cat <<'EOF'
Usage:
  ./scripts/deploy.sh [OPTIONS]

Options:
  --host NAME              Deploy only to this host (can be repeated)
  --release-id ID          Explicit release ID (default: timestamp-gitsha)
  --skip-build             Skip local build phase
  --skip-frontend          Skip frontend staging and deploy
  --skip-backend           Skip backend binary staging and deploy
  --skip-services          Skip service restarts
  --prepare-only           Run preflight + prepare, do not activate
  --activate-only          Activate previously prepared release
  --resume                 Resume interrupted deployment (skip prepared/active phases)
  --status                 Show release status per host, no changes
  --canary                 Deploy only canary hosts
  --canary-then-fleet      Deploy canary hosts first, then remaining hosts
  --health-timeout SEC     Health check timeout after activation (default: 90)
  --min-free-mb MB         Minimum free disk required for preflight (default: 1024)
  --dry-run                Print actions without executing
  --config FILE            Use alternate hosts config
  --help                   Show this help
EOF
    exit 0
}

declare -a HOST_FILTER=()
declare -a H_NAME=() H_SSH=() H_MODE=() H_USER=() H_FRONTEND=() H_WEB_ROOT=()
declare -a H_BINARIES=() H_SERVICES=() H_LOCAL=() H_CANARY=()
declare -a REQUIRED_DEP_BINARIES=()
declare -A REQUIRED_DEP_VERSIONS=()

# --- Argument parsing ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --host) HOST_FILTER+=("$2"); shift 2 ;;
        --release-id) RELEASE_ID="$2"; shift 2 ;;
        --skip-build) SKIP_BUILD=true; shift ;;
        --skip-frontend) SKIP_FRONTEND=true; shift ;;
        --skip-backend) SKIP_BACKEND=true; shift ;;
        --skip-services) SKIP_SERVICES=true; shift ;;
        --prepare-only) PREPARE_ONLY=true; shift ;;
        --activate-only) ACTIVATE_ONLY=true; shift ;;
        --resume) RESUME=true; shift ;;
        --status) STATUS_ONLY=true; shift ;;
        --canary) CANARY_ONLY=true; shift ;;
        --canary-then-fleet) CANARY_THEN_FLEET=true; shift ;;
        --health-timeout) HEALTH_TIMEOUT_SECONDS="$2"; shift 2 ;;
        --min-free-mb) MIN_FREE_MB="$2"; shift 2 ;;
        --dry-run) DRY_RUN=true; shift ;;
        --config) CONFIG="$2"; shift 2 ;;
        --help|-h) usage ;;
        *) err "Unknown option: $1"; usage ;;
    esac
done

if [[ ! -f "$CONFIG" ]]; then
    err "Config not found: $CONFIG"
    exit 1
fi

if [[ "$PREPARE_ONLY" == "true" && "$ACTIVATE_ONLY" == "true" ]]; then
    err "--prepare-only and --activate-only are mutually exclusive"
    exit 1
fi

if [[ "$CANARY_ONLY" == "true" && "$CANARY_THEN_FLEET" == "true" ]]; then
    err "--canary and --canary-then-fleet are mutually exclusive"
    exit 1
fi

if [[ -z "$RELEASE_ID" ]]; then
    git_sha="$(git -C "$ROOT_DIR" rev-parse --short=10 HEAD 2>/dev/null || echo nogit)"
    RELEASE_ID="$(date +%Y%m%d%H%M%S)-${git_sha}"
fi

# Validate hosts config TOML once up front.
if ! python3 - <<PY >/dev/null 2>&1
import tomllib
with open("$CONFIG", "rb") as f:
    tomllib.load(f)
PY
then
    err "Invalid TOML: $CONFIG"
    exit 1
fi

host_exec() {
    local is_local="$1"
    local ssh_target="$2"
    local cmd="$3"
    if [[ "$DRY_RUN" == "true" ]]; then
        if [[ "$is_local" == "true" ]]; then
            echo -e "${YELLOW}  [dry-run]${NC} local :: $cmd"
        else
            echo -e "${YELLOW}  [dry-run]${NC} ssh $ssh_target :: $cmd"
        fi
        return 0
    fi

    if [[ "$is_local" == "true" ]]; then
        bash -lc "$cmd"
    else
        ssh "$ssh_target" "bash -lc $(printf '%q' "$cmd")"
    fi
}

host_exec_sudo() {
    local is_local="$1"
    local ssh_target="$2"
    local inner="$3"
    host_exec "$is_local" "$ssh_target" "sudo bash -lc $(printf '%q' "$inner")"
}

emit_event() {
    local is_local="$1"
    local ssh_target="$2"
    local host_name="$3"
    local phase="$4"
    local result="$5"
    local reason_code="$6"

    local actor="${USER:-unknown}"
    local ts
    ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    local payload
    payload="$(python3 - <<PY
import json
print(json.dumps({
  "timestamp": "$ts",
  "release_id": "$RELEASE_ID",
  "host": "$host_name",
  "actor": "$actor",
  "phase": "$phase",
  "result": "$result",
  "reason_code": "$reason_code"
}, separators=(",", ":")))
PY
)"

    local escaped
    escaped="$(printf '%q' "$payload")"
    host_exec_sudo "$is_local" "$ssh_target" "mkdir -p '$(dirname "$EVENT_LOG_PATH")' && printf '%s\\n' $escaped >> '$EVENT_LOG_PATH'" || true
}

normalize_mode() {
    local mode="$1"
    case "$mode" in
        local|single-user) echo "single-user" ;;
        multi-user) echo "multi-user" ;;
        *) echo "invalid" ;;
    esac
}

version_ge() {
    local current="$1"
    local required="$2"
    [[ "$(printf '%s\n%s\n' "$required" "$current" | sort -V | head -n1)" == "$required" ]]
}

load_dependency_requirements() {
    if [[ ! -f "$DEPENDENCY_POLICY_FILE" ]]; then
        warn "Dependency policy file missing: $DEPENDENCY_POLICY_FILE"
        return 0
    fi

    while IFS='=' read -r bin ver; do
        [[ -z "$bin" || -z "$ver" ]] && continue
        REQUIRED_DEP_BINARIES+=("$bin")
        REQUIRED_DEP_VERSIONS["$bin"]="$ver"
    done < <(python3 - <<PY
import tomllib
from pathlib import Path

p = Path(r"$DEPENDENCY_POLICY_FILE")
data = tomllib.loads(p.read_text())
byteowlz = data.get("byteowlz", {})
# Deploy/runtime-critical CLI dependencies.
keys = ("eavs", "hstry", "mmry", "trx", "agntz", "sx", "skdlr")
for key in keys:
    v = str(byteowlz.get(key, "")).strip()
    if not v or v == "latest":
        continue
    print(f"{key}={v}")
PY
)
}

check_dependency_compatibility() {
    local name="$1" ssh_target="$2" is_local="$3"

    if [[ "${#REQUIRED_DEP_BINARIES[@]}" -eq 0 ]]; then
        return 0
    fi

    if [[ "$DRY_RUN" == "true" ]]; then
        local dep
        for dep in "${REQUIRED_DEP_BINARIES[@]}"; do
            echo -e "${YELLOW}  [dry-run]${NC} dependency gate: $dep >= ${REQUIRED_DEP_VERSIONS[$dep]}"
        done
        return 0
    fi

    local dep required current
    for dep in "${REQUIRED_DEP_BINARIES[@]}"; do
        required="${REQUIRED_DEP_VERSIONS[$dep]}"

        if ! host_exec "$is_local" "$ssh_target" "command -v '$dep' >/dev/null"; then
            emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "deps.${dep}.missing"
            err "Missing dependency '$dep' on $name"
            return 1
        fi

        current="$(host_exec "$is_local" "$ssh_target" "$dep --version 2>/dev/null | grep -oE '[0-9]+\\.[0-9]+\\.[0-9]+' | head -1")"
        if [[ -z "$current" ]]; then
            emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "deps.${dep}.version_unknown"
            err "Could not detect $dep version on $name"
            return 1
        fi

        if ! version_ge "$current" "$required"; then
            emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "deps.${dep}.version_mismatch"
            err "Dependency version mismatch on $name: $dep=$current, required >= $required"
            return 1
        fi
    done

    # Extra compatibility guard: hstry adapters CLI must be functional.
    if ! host_exec "$is_local" "$ssh_target" "hstry adapters --help >/dev/null 2>&1"; then
        emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "deps.hstry.adapters_unavailable"
        err "hstry adapters command unavailable on $name"
        return 1
    fi

    return 0
}

parse_hosts() {
    local idx=-1
    local in_host=false

    while IFS= read -r line; do
        line="${line%%#*}"
        line="$(echo "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
        [[ -z "$line" ]] && continue

        if [[ "$line" == "[[host]]" ]]; then
            idx=$((idx + 1))
            in_host=true
            H_NAME[$idx]=""
            H_SSH[$idx]=""
            H_MODE[$idx]="single-user"
            H_USER[$idx]=""
            H_FRONTEND[$idx]="false"
            H_WEB_ROOT[$idx]=""
            H_BINARIES[$idx]=""
            H_SERVICES[$idx]=""
            H_LOCAL[$idx]="false"
            H_CANARY[$idx]="false"
            continue
        fi

        if [[ "$in_host" != "true" ]]; then
            continue
        fi

        local key val
        key="$(echo "$line" | sed 's/[[:space:]]*=.*//')"
        val="$(echo "$line" | sed 's/[^=]*=[[:space:]]*//')"
        val="$(echo "$val" | sed 's/^"//;s/"$//')"

        case "$key" in
            name) H_NAME[$idx]="$val" ;;
            ssh) H_SSH[$idx]="$val" ;;
            mode) H_MODE[$idx]="$val" ;;
            user) H_USER[$idx]="$val" ;;
            frontend) H_FRONTEND[$idx]="$val" ;;
            web_root) H_WEB_ROOT[$idx]="$val" ;;
            local) H_LOCAL[$idx]="$val" ;;
            canary) H_CANARY[$idx]="$val" ;;
            binaries)
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
load_dependency_requirements
HOST_COUNT="${#H_NAME[@]}"
if [[ "$HOST_COUNT" -eq 0 ]]; then
    err "No hosts found in $CONFIG"
    exit 1
fi

should_deploy() {
    local name="$1"
    if [[ ${#HOST_FILTER[@]} -eq 0 ]]; then
        return 0
    fi
    for f in "${HOST_FILTER[@]}"; do
        [[ "$f" == "$name" ]] && return 0
    done
    return 1
}

is_canary_host() {
    local index="$1"
    if [[ "${H_CANARY[$index]}" == "true" ]]; then
        return 0
    fi
    return 1
}

preflight_host() {
    local name="$1" ssh_target="$2" mode="$3" is_local="$4" binaries="$5"

    emit_event "$is_local" "$ssh_target" "$name" "preflight" "start" "preflight.start"

    if [[ "$is_local" != "true" ]]; then
        if [[ "$DRY_RUN" == "true" ]]; then
            echo -e "${YELLOW}  [dry-run]${NC} ssh connectivity check: $ssh_target"
        else
            log "Checking SSH connectivity to $ssh_target..."
            if ! ssh -o ConnectTimeout=5 "$ssh_target" "echo ok" &>/dev/null; then
                emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "ssh.unreachable"
                err "Cannot reach $ssh_target"
                return 1
            fi
        fi
    fi

    if [[ "$mode" == "invalid" ]]; then
        emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "mode.invalid"
        err "Host $name has invalid mode"
        return 1
    fi

    if [[ "$SKIP_BACKEND" != "true" ]]; then
        local bin
        for bin in $binaries; do
            if [[ ! -f "$ROOT_DIR/backend/target/release/$bin" ]]; then
                emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "binary.missing.$bin"
                err "Missing local build artifact: backend/target/release/$bin"
                return 1
            fi
        done
    fi

    local disk_cmd
    disk_cmd="free_mb=\$(df -Pm '$RELEASES_ROOT' 2>/dev/null | awk 'NR==2{print \$4}'); if [[ -z \"\$free_mb\" ]]; then free_mb=\$(df -Pm /var/lib | awk 'NR==2{print \$4}'); fi; [[ \$free_mb -ge $MIN_FREE_MB ]]"
    if ! host_exec_sudo "$is_local" "$ssh_target" "$disk_cmd"; then
        emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "disk.low"
        err "Preflight disk check failed on $name"
        return 1
    fi

    if ! host_exec "$is_local" "$ssh_target" "command -v systemctl >/dev/null && command -v install >/dev/null"; then
        emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "deps.missing"
        err "Missing required runtime dependencies on $name"
        return 1
    fi

    if ! check_dependency_compatibility "$name" "$ssh_target" "$is_local"; then
        return 1
    fi

    if [[ "$mode" == "multi-user" ]]; then
        if [[ "$DRY_RUN" == "true" ]]; then
            echo -e "${YELLOW}  [dry-run]${NC} multi-user sandbox/seccomp preflight checks"
            emit_event "$is_local" "$ssh_target" "$name" "preflight" "pass" "preflight.pass"
            ok "Preflight passed on $name"
            return 0
        fi

        local sandbox_stat_cmd
        sandbox_stat_cmd="test -f /etc/oqto/sandbox.toml && test -r /etc/oqto/sandbox.toml && stat -c '%U:%G %a' /etc/oqto/sandbox.toml"
        local sandbox_stat
        if ! sandbox_stat="$(host_exec_sudo "$is_local" "$ssh_target" "$sandbox_stat_cmd")"; then
            emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "sandbox.missing"
            err "Missing or unreadable /etc/oqto/sandbox.toml on $name"
            return 1
        fi

        local owner perm
        owner="$(echo "$sandbox_stat" | awk '{print $1}')"
        perm="$(echo "$sandbox_stat" | awk '{print $2}')"
        if [[ "$owner" != "root:root" ]]; then
            emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "sandbox.owner"
            err "sandbox.toml must be owned by root:root on $name"
            return 1
        fi
        if [[ "$perm" -gt 644 ]]; then
            emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "sandbox.perms"
            err "sandbox.toml permissions too open ($perm) on $name"
            return 1
        fi

        local seccomp_enforced
        seccomp_enforced="$(host_exec_sudo "$is_local" "$ssh_target" "python3 - <<'PY'
from pathlib import Path
p=Path('/etc/oqto/sandbox.toml')
text=p.read_text(errors='ignore') if p.exists() else ''
needle=('seccomp_enforce = true','seccomp_mode = \"enforce\"','seccomp = \"enforce\"')
print('true' if any(n in text for n in needle) else 'false')
PY
")"
        if [[ "$seccomp_enforced" == "true" ]]; then
            if ! host_exec_sudo "$is_local" "$ssh_target" "test -r /etc/oqto/seccomp/default.bpf"; then
                emit_event "$is_local" "$ssh_target" "$name" "preflight" "fail" "seccomp.bpf.missing"
                err "seccomp enforce configured but /etc/oqto/seccomp/default.bpf missing on $name"
                return 1
            fi
        fi
    fi

    emit_event "$is_local" "$ssh_target" "$name" "preflight" "pass" "preflight.pass"
    ok "Preflight passed on $name"
}

prepare_host() {
    local name="$1" ssh_target="$2" is_local="$3" binaries="$4" frontend="$5" web_root="$6"
    local release_dir="$RELEASES_ROOT/$RELEASE_ID"

    emit_event "$is_local" "$ssh_target" "$name" "deploy.prepare" "start" "deploy.prepare.start"

    if [[ "$RESUME" == "true" ]]; then
        if host_exec_sudo "$is_local" "$ssh_target" "test -f '$release_dir/.prepared'"; then
            log "Prepare already completed on $name (resume mode)"
            emit_event "$is_local" "$ssh_target" "$name" "deploy.prepare" "pass" "prepare.resume.skip"
            return 0
        fi
    fi

    host_exec_sudo "$is_local" "$ssh_target" "mkdir -p '$release_dir/bin' '$release_dir/meta'"

    if [[ "$SKIP_BACKEND" != "true" ]]; then
        local bin
        for bin in $binaries; do
            local src="$ROOT_DIR/backend/target/release/$bin"
            if [[ "$is_local" == "true" ]]; then
                host_exec_sudo "$is_local" "$ssh_target" "install -m 0755 '$src' '$release_dir/bin/$bin'"
            else
                if [[ "$DRY_RUN" == "true" ]]; then
                    echo -e "${YELLOW}  [dry-run]${NC} scp $src $ssh_target:/tmp/oqto-${bin}-${RELEASE_ID}"
                else
                    local tmp_remote="/tmp/oqto-${bin}-${RELEASE_ID}"
                    scp -q "$src" "$ssh_target:$tmp_remote"
                    host_exec_sudo "$is_local" "$ssh_target" "install -m 0755 '$tmp_remote' '$release_dir/bin/$bin' && rm -f '$tmp_remote'"
                fi
            fi
        done
    fi

    if [[ "$SKIP_FRONTEND" != "true" && "$frontend" == "true" ]]; then
        local dist_dir="$ROOT_DIR/frontend/dist"
        if [[ -d "$dist_dir" ]]; then
            host_exec_sudo "$is_local" "$ssh_target" "mkdir -p '$release_dir/frontend'"
            if [[ "$is_local" == "true" ]]; then
                host_exec_sudo "$is_local" "$ssh_target" "rsync -a --delete '$dist_dir/' '$release_dir/frontend/'"
            else
                if [[ "$DRY_RUN" == "true" ]]; then
                    echo -e "${YELLOW}  [dry-run]${NC} rsync -az --delete $dist_dir/ $ssh_target:$release_dir/frontend/"
                else
                    local tmp_frontend="/tmp/oqto-frontend-${RELEASE_ID}"
                    ssh "$ssh_target" "rm -rf '$tmp_frontend' && mkdir -p '$tmp_frontend'"
                    rsync -az --delete "$dist_dir/" "$ssh_target:$tmp_frontend/"
                    host_exec_sudo "$is_local" "$ssh_target" "mkdir -p '$release_dir/frontend' && rsync -a --delete '$tmp_frontend/' '$release_dir/frontend/' && rm -rf '$tmp_frontend'"
                fi
            fi
        fi
        host_exec_sudo "$is_local" "$ssh_target" "printf '%s\n' '$web_root' > '$release_dir/meta/web_root'"
    fi

    host_exec_sudo "$is_local" "$ssh_target" "printf '%s\n' '$RELEASE_ID' > '$release_dir/meta/release_id' && touch '$release_dir/.prepared'"
    emit_event "$is_local" "$ssh_target" "$name" "deploy.prepare" "pass" "deploy.prepare.pass"
    ok "Prepared release $RELEASE_ID on $name"
}

install_current_symlinks() {
    local is_local="$1" ssh_target="$2" binaries="$3"
    local current_link="$RELEASES_ROOT/current"
    local bin
    for bin in $binaries; do
        host_exec_sudo "$is_local" "$ssh_target" "ln -sfn '$current_link/bin/$bin' '/usr/local/bin/$bin'"
    done
}

restart_services_ordered() {
    local is_local="$1" ssh_target="$2" mode="$3" services="$4"

    if [[ "$SKIP_SERVICES" == "true" ]]; then
        return 0
    fi

    # Ordered restarts: runner -> control plane (oqto) -> everything else.
    if [[ "$mode" == "single-user" ]]; then
        host_exec "$is_local" "$ssh_target" "systemctl --user restart oqto-runner" || true
    fi

    host_exec_sudo "$is_local" "$ssh_target" "systemctl restart oqto" || true

    local svc
    for svc in $services; do
        if [[ "$svc" == "oqto" || "$svc" == "oqto-runner" ]]; then
            continue
        fi
        host_exec_sudo "$is_local" "$ssh_target" "systemctl restart '$svc'" || true
    done

    if [[ "$mode" == "multi-user" ]]; then
        host_exec "$is_local" "$ssh_target" "oqtoctl user sync-configs" || true
    fi
}

health_check_host() {
    local is_local="$1" ssh_target="$2" mode="$3"
    local start
    start="$(date +%s)"

    while true; do
        local elapsed
        elapsed=$(( $(date +%s) - start ))
        if [[ "$elapsed" -ge "$HEALTH_TIMEOUT_SECONDS" ]]; then
            return 1
        fi

        local ok_backend="false" ok_runner="false" ok_deps="false"

        if host_exec "$is_local" "$ssh_target" "curl -sf http://127.0.0.1:8080/api/health >/dev/null"; then
            ok_backend="true"
        fi

        if [[ "$mode" == "single-user" ]]; then
            if host_exec "$is_local" "$ssh_target" "uid=\$(id -u); test -S /run/user/\${uid}/oqto-runner.sock"; then
                ok_runner="true"
            fi
        else
            ok_runner="true"
        fi

        if host_exec "$is_local" "$ssh_target" "command -v hstry >/dev/null && command -v mmry >/dev/null"; then
            ok_deps="true"
        fi

        if [[ "$ok_backend" == "true" && "$ok_runner" == "true" && "$ok_deps" == "true" ]]; then
            return 0
        fi

        sleep 2
    done
}

rollback_host() {
    local name="$1" ssh_target="$2" is_local="$3" binaries="$4" mode="$5" services="$6"
    local previous_release="$7"

    emit_event "$is_local" "$ssh_target" "$name" "rollback" "start" "rollback.start"

    if [[ -z "$previous_release" ]]; then
        emit_event "$is_local" "$ssh_target" "$name" "rollback" "fail" "rollback.no_previous_release"
        err "Rollback failed on $name: no previous release"
        return 1
    fi

    host_exec_sudo "$is_local" "$ssh_target" "ln -sfn '$RELEASES_ROOT/$previous_release' '$RELEASES_ROOT/current'"
    install_current_symlinks "$is_local" "$ssh_target" "$binaries"
    restart_services_ordered "$is_local" "$ssh_target" "$mode" "$services"

    if health_check_host "$is_local" "$ssh_target" "$mode"; then
        host_exec_sudo "$is_local" "$ssh_target" "ln -sfn '$RELEASES_ROOT/$previous_release' '$RELEASES_ROOT/last-good'"
        emit_event "$is_local" "$ssh_target" "$name" "rollback" "pass" "rollback.pass"
        ok "Rollback succeeded on $name"
        return 0
    fi

    emit_event "$is_local" "$ssh_target" "$name" "rollback" "fail" "rollback.health_failed"
    err "Rollback health checks failed on $name"
    return 1
}

activate_host() {
    local name="$1" ssh_target="$2" is_local="$3" mode="$4" binaries="$5" services="$6" frontend="$7" web_root="$8"
    local release_dir="$RELEASES_ROOT/$RELEASE_ID"

    emit_event "$is_local" "$ssh_target" "$name" "deploy.activate" "start" "deploy.activate.start"

    if ! host_exec_sudo "$is_local" "$ssh_target" "test -f '$release_dir/.prepared'"; then
        emit_event "$is_local" "$ssh_target" "$name" "deploy.activate" "fail" "activate.not_prepared"
        err "Release $RELEASE_ID not prepared on $name"
        return 1
    fi

    if [[ "$RESUME" == "true" ]]; then
        if host_exec_sudo "$is_local" "$ssh_target" "[[ \"\$(readlink -f '$RELEASES_ROOT/current' 2>/dev/null || true)\" == \"$release_dir\" ]]"; then
            log "Activation already applied on $name (resume mode)"
            if health_check_host "$is_local" "$ssh_target" "$mode"; then
                emit_event "$is_local" "$ssh_target" "$name" "deploy.activate" "pass" "activate.resume.skip"
                return 0
            fi
        fi
    fi

    local previous_release
    previous_release="$(host_exec_sudo "$is_local" "$ssh_target" "basename \"\$(readlink -f '$RELEASES_ROOT/current' 2>/dev/null || true)\"")"

    host_exec_sudo "$is_local" "$ssh_target" "ln -sfn '$release_dir' '$RELEASES_ROOT/current'"
    install_current_symlinks "$is_local" "$ssh_target" "$binaries"

    if [[ "$SKIP_FRONTEND" != "true" && "$frontend" == "true" ]]; then
        host_exec_sudo "$is_local" "$ssh_target" "mkdir -p '$web_root' && rsync -a --delete '$release_dir/frontend/' '$web_root/'"
    fi

    restart_services_ordered "$is_local" "$ssh_target" "$mode" "$services"

    if health_check_host "$is_local" "$ssh_target" "$mode"; then
        host_exec_sudo "$is_local" "$ssh_target" "ln -sfn '$release_dir' '$RELEASES_ROOT/last-good'"
        emit_event "$is_local" "$ssh_target" "$name" "deploy.activate" "pass" "deploy.activate.pass"
        ok "Activated release $RELEASE_ID on $name"
        return 0
    fi

    emit_event "$is_local" "$ssh_target" "$name" "deploy.activate" "fail" "health.failed"
    warn "Health check failed on $name, attempting rollback..."
    rollback_host "$name" "$ssh_target" "$is_local" "$binaries" "$mode" "$services" "$previous_release"
}

print_status_host() {
    local name="$1" ssh_target="$2" is_local="$3"
    local current last_good prepared

    if [[ "$DRY_RUN" == "true" ]]; then
        log "[$name] release_id=$RELEASE_ID prepared=? current=? last_good=? (dry-run)"
        return 0
    fi

    current="$(host_exec_sudo "$is_local" "$ssh_target" "basename \"\$(readlink -f '$RELEASES_ROOT/current' 2>/dev/null || true)\"")"
    last_good="$(host_exec_sudo "$is_local" "$ssh_target" "basename \"\$(readlink -f '$RELEASES_ROOT/last-good' 2>/dev/null || true)\"")"
    prepared="$(host_exec_sudo "$is_local" "$ssh_target" "test -f '$RELEASES_ROOT/$RELEASE_ID/.prepared' && echo yes || echo no")"

    log "[$name] release_id=$RELEASE_ID prepared=$prepared current=${current:-none} last_good=${last_good:-none}"
}

build_artifacts() {
    if [[ "$SKIP_BUILD" == "true" ]]; then
        return 0
    fi

    log "Building artifacts..."
    if [[ "$SKIP_BACKEND" != "true" ]]; then
        host_exec "true" "" "cd '$ROOT_DIR/backend' && remote-build build --release -p oqto --bin oqto --bin oqto-sandbox"
        host_exec "true" "" "cd '$ROOT_DIR/backend' && remote-build build --release -p oqto-runner --bin oqto-runner"
        host_exec "true" "" "cd '$ROOT_DIR/backend' && remote-build build --release -p oqto-files --bin oqto-files"
        host_exec "true" "" "cd '$ROOT_DIR/backend' && remote-build build --release -p oqto-usermgr --bin oqto-usermgr"
    fi

    if [[ "$SKIP_FRONTEND" != "true" ]]; then
        host_exec "true" "" "cd '$ROOT_DIR/frontend' && bun run build"
    fi

    ok "Build completed"
}

deploy_host() {
    local i="$1"
    local name="${H_NAME[$i]}"
    local ssh_target="${H_SSH[$i]}"
    local mode
    mode="$(normalize_mode "${H_MODE[$i]}")"
    local frontend="${H_FRONTEND[$i]}"
    local web_root="${H_WEB_ROOT[$i]}"
    local binaries="${H_BINARIES[$i]}"
    local services="${H_SERVICES[$i]}"
    local is_local="${H_LOCAL[$i]}"

    echo ""
    log "=========================================="
    log "Deploying ${BOLD}$name${NC} release ${BOLD}$RELEASE_ID${NC}"
    log "=========================================="

    if ! preflight_host "$name" "$ssh_target" "$mode" "$is_local" "$binaries"; then
        return 1
    fi

    if [[ "$STATUS_ONLY" == "true" ]]; then
        print_status_host "$name" "$ssh_target" "$is_local"
        return 0
    fi

    if [[ "$ACTIVATE_ONLY" != "true" ]]; then
        if ! prepare_host "$name" "$ssh_target" "$is_local" "$binaries" "$frontend" "$web_root"; then
            return 1
        fi
    fi

    if [[ "$PREPARE_ONLY" != "true" ]]; then
        if ! activate_host "$name" "$ssh_target" "$is_local" "$mode" "$binaries" "$services" "$frontend" "$web_root"; then
            return 1
        fi
    fi

    ok "Deployment finished for $name"
}

collect_targets() {
    local include_canary="$1"
    local include_non_canary="$2"
    local -n out_ref=$3

    out_ref=()
    local i
    for ((i=0; i<HOST_COUNT; i++)); do
        should_deploy "${H_NAME[$i]}" || continue

        if is_canary_host "$i"; then
            [[ "$include_canary" == "true" ]] || continue
        else
            [[ "$include_non_canary" == "true" ]] || continue
        fi

        out_ref+=("$i")
    done
}

run_targets() {
    local -n target_ref=$1
    local i
    for i in "${target_ref[@]}"; do
        if ! deploy_host "$i"; then
            err "Deployment failed for ${H_NAME[$i]}"
            return 1
        fi
    done
}

build_artifacts

if [[ "$CANARY_THEN_FLEET" == "true" ]]; then
    declare -a canary_targets fleet_targets
    collect_targets "true" "false" canary_targets
    if [[ "${#canary_targets[@]}" -eq 0 ]]; then
        err "No canary hosts selected"
        exit 1
    fi

    log "Starting canary deployment..."
    run_targets canary_targets
    ok "Canary deployment passed"

    collect_targets "false" "true" fleet_targets
    if [[ "${#fleet_targets[@]}" -gt 0 ]]; then
        log "Starting fleet rollout..."
        run_targets fleet_targets
    fi
elif [[ "$CANARY_ONLY" == "true" ]]; then
    declare -a canary_targets
    collect_targets "true" "false" canary_targets
    if [[ "${#canary_targets[@]}" -eq 0 ]]; then
        err "No canary hosts selected"
        exit 1
    fi
    run_targets canary_targets
else
    declare -a all_targets
    collect_targets "true" "true" all_targets
    if [[ "${#all_targets[@]}" -eq 0 ]]; then
        err "No hosts matched filters"
        exit 1
    fi
    run_targets all_targets
fi

ok "=========================================="
ok "Deployment complete (release: $RELEASE_ID)"
ok "=========================================="
