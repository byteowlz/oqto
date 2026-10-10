#!/usr/bin/env bash
# Oqto deployment driver (ADR-0018, ADR-0056).
#
# Thin host iteration only. Every stateful step runs on the target inside one
# root transaction (scripts/dist/deploy-transaction.py) that calls the single
# activation engine, `oqto-setup install`, with an in-transaction service
# check. A failed check is rolled back by oqto-setup before last-good moves.
#
#   build   -> clean worktree only; artifact version == Cargo version
#   install -> provenance/downgrade gates in oqto-setup; protected snapshots
#   history -> never during activation; `--backfill`/`--backfill-only` run the
#              separate owner-correct, snapshot-protected backfill transaction

# Remote command strings are assembled locally on purpose; every interpolated
# value is either regex-validated or quoted with printf %q first.
# shellcheck disable=SC2029
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="${ROOT_DIR}/deploy/hosts.toml"
DEPENDENCY_POLICY_FILE="$ROOT_DIR/dependencies.toml"
TRANSACTION="$ROOT_DIR/scripts/dist/deploy-transaction.py"
BOOTSTRAP="$ROOT_DIR/scripts/dist/verified-setup-bootstrap.py"
CANONICAL_WEB_ROOT="/var/www/oqto"
DRY_RUN=false
STATUS_ONLY=false
CANARY_ONLY=false
CANARY_THEN_FLEET=false
DEPLOY_ARTIFACT=""
DEPLOY_CHECKSUM=""
DEPLOY_MODE="release"
HEALTH_TIMEOUT_SECONDS=180
MIN_FREE_MB=1024
KEEP_RELEASES="${OQTO_KEEP_RELEASES:-3}"
ALLOW_DOWNGRADE=false
ALLOW_DIRTY_SOURCE=false
ALLOW_BUSY_SESSIONS=false
SYNC_EXTENSIONS=true
BACKFILL=false
BACKFILL_ONLY=false
BACKFILL_MODES="sync-identities,bootstrap,validate"
declare -a BACKFILL_PRINCIPALS=()
declare -a HOST_FILTER=()

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

# OpenSSH 10+ warns on every connection about weak KEX; deploy opens many.
declare -a OQTO_SSH_OPTIONS=()
if [[ "${OQTO_SUPPRESS_SSH_PQ_WARNING:-true}" == "true" ]] \
    && command ssh -G -o WarnWeakCrypto=no example.invalid >/dev/null 2>&1; then
    OQTO_SSH_OPTIONS+=("-o" "WarnWeakCrypto=no")
fi
ssh() { command ssh "${OQTO_SSH_OPTIONS[@]}" "$@"; }
scp() { command scp "${OQTO_SSH_OPTIONS[@]}" "$@"; }

usage() {
    cat <<'EOF'
Usage:
  ./scripts/deploy.sh [OPTIONS]

Release selection:
  --artifact FILE          Deploy this canonical release archive
  --checksum FILE          Independent SHA-256 file (required with --artifact)
                           Without --artifact, a release is built from a clean
                           checkout at the declared Cargo version.
Hosts:
  --host NAME              Deploy only to this host (repeatable)
  --config FILE            Alternate hosts config (default: deploy/hosts.toml)
  --canary                 Deploy only canary hosts
  --canary-then-fleet      Canary hosts first, then the rest
  --status                 Show active/last-good releases, change nothing
  --dry-run                Print the plan, change nothing
Policy:
  --mode MODE              Tool dependency policy: release (default) or dev
  --allow-downgrade        Install a release not newer than the active one
  --allow-dirty-source     Install a release recorded as built from a dirty tree
  --allow-busy-sessions    Proceed while agent sessions are streaming
  --health-timeout SEC     Post-activation health budget (default: 180)
  --keep-releases N        Superseded releases kept (default: 3; 0 disables)
  --min-free-mb MB         Required free disk for preflight (default: 1024)
  --skip-extensions        Do not sync the pinned Pi extension set
History (never part of activation):
  --backfill               After activation, run the protected oqto-log backfill
  --backfill-only          Only run the protected backfill; no binaries change
  --backfill-modes LIST    Comma list from sync-identities,bootstrap,validate,
                           validate-changed (default: sync-identities,bootstrap,validate)
  --backfill-principal U   Restrict backfill to this principal (repeatable)
  --help                   Show this help
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --host) HOST_FILTER+=("$2"); shift 2 ;;
        --config) CONFIG="$2"; shift 2 ;;
        --mode) DEPLOY_MODE="$2"; shift 2 ;;
        --artifact) DEPLOY_ARTIFACT="$2"; shift 2 ;;
        --checksum) DEPLOY_CHECKSUM="$2"; shift 2 ;;
        --canary) CANARY_ONLY=true; shift ;;
        --canary-then-fleet) CANARY_THEN_FLEET=true; shift ;;
        --status) STATUS_ONLY=true; shift ;;
        --dry-run) DRY_RUN=true; shift ;;
        --allow-downgrade) ALLOW_DOWNGRADE=true; shift ;;
        --allow-dirty-source) ALLOW_DIRTY_SOURCE=true; shift ;;
        --allow-busy-sessions) ALLOW_BUSY_SESSIONS=true; shift ;;
        --health-timeout) HEALTH_TIMEOUT_SECONDS="$2"; shift 2 ;;
        --keep-releases) KEEP_RELEASES="$2"; shift 2 ;;
        --min-free-mb) MIN_FREE_MB="$2"; shift 2 ;;
        --skip-extensions) SYNC_EXTENSIONS=false; shift ;;
        --backfill) BACKFILL=true; shift ;;
        --backfill-only) BACKFILL_ONLY=true; shift ;;
        --backfill-modes) BACKFILL_MODES="$2"; shift 2 ;;
        --backfill-principal) BACKFILL_PRINCIPALS+=("$2"); shift 2 ;;
        --help|-h) usage ;;
        *) err "Unknown option: $1 (legacy prepare/activate/build flags were removed; see --help)"; exit 2 ;;
    esac
done

fail() { err "$*"; exit 1; }

case "$DEPLOY_MODE" in
    release|dev) ;;
    *) fail "Invalid --mode '$DEPLOY_MODE' (expected: release or dev)" ;;
esac
[[ -f "$CONFIG" ]] || fail "Config not found: $CONFIG"
[[ "$CANARY_ONLY" == true && "$CANARY_THEN_FLEET" == true ]] \
    && fail "--canary and --canary-then-fleet are mutually exclusive"
[[ "$BACKFILL" == true && "$BACKFILL_ONLY" == true ]] \
    && fail "--backfill and --backfill-only are mutually exclusive"
[[ "$HEALTH_TIMEOUT_SECONDS" =~ ^[0-9]+$ && "$KEEP_RELEASES" =~ ^[0-9]+$ && "$MIN_FREE_MB" =~ ^[0-9]+$ ]] \
    || fail "--health-timeout, --keep-releases and --min-free-mb must be non-negative integers"
[[ "$BACKFILL_MODES" =~ ^(sync-identities|bootstrap|validate|validate-changed)(,(sync-identities|bootstrap|validate|validate-changed))*$ ]] \
    || fail "--backfill-modes must list only sync-identities, bootstrap, validate, validate-changed"
for principal in "${BACKFILL_PRINCIPALS[@]}"; do
    [[ "$principal" =~ ^[a-z_][a-z0-9_-]*$ ]] || fail "invalid --backfill-principal: $principal"
done
if [[ -n "$DEPLOY_ARTIFACT" ]]; then
    [[ -f "$DEPLOY_ARTIFACT" ]] || fail "Artifact not found: $DEPLOY_ARTIFACT"
    [[ -n "$DEPLOY_CHECKSUM" && -f "$DEPLOY_CHECKSUM" ]] \
        || fail "--artifact requires an existing --checksum file from a trusted independent source"
elif [[ -n "$DEPLOY_CHECKSUM" ]]; then
    fail "--checksum requires an explicit --artifact"
fi

# --- hosts ------------------------------------------------------------------

declare -a H_NAME=() H_SSH=() H_MODE=() H_USER=() H_FRONTEND=() H_WEB_ROOT=() H_LOCAL=() H_CANARY=()

parse_hosts() {
    local rows
    rows="$(python3 - "$CONFIG" <<'PY'
import sys, tomllib
hosts = tomllib.load(open(sys.argv[1], "rb")).get("host", [])
for host in hosts:
    fields = [host.get("name", ""), host.get("ssh", ""), host.get("mode", "single-user"),
              host.get("user", ""), str(host.get("frontend", False)).lower(),
              host.get("web_root", ""), str(host.get("local", False)).lower(),
              str(host.get("canary", False)).lower()]
    # Unit separator: unlike tab it is not IFS whitespace, so empty fields survive.
    if any("\x1f" in f or "\n" in f for f in fields):
        raise SystemExit("host fields must not contain control separators or newlines")
    print("\x1f".join(fields))
PY
)" || fail "Invalid hosts config: $CONFIG"
    local name ssh_target mode user frontend web_root is_local canary
    while IFS=$'\x1f' read -r name ssh_target mode user frontend web_root is_local canary; do
        [[ -n "$name" ]] || continue
        H_NAME+=("$name"); H_SSH+=("$ssh_target"); H_MODE+=("$mode"); H_USER+=("$user")
        H_FRONTEND+=("$frontend"); H_WEB_ROOT+=("$web_root"); H_LOCAL+=("$is_local"); H_CANARY+=("$canary")
    done <<<"$rows"
    [[ "${#H_NAME[@]}" -gt 0 ]] || fail "No hosts found in $CONFIG"
}

profile_for() {
    case "$1" in
        local|single-user) echo personal ;;
        multi-user) echo team ;;
        *) echo invalid ;;
    esac
}

should_deploy() {
    local name="$1" filter
    [[ "${#HOST_FILTER[@]}" -eq 0 ]] && return 0
    for filter in "${HOST_FILTER[@]}"; do [[ "$filter" == "$name" ]] && return 0; done
    return 1
}

collect_targets() {
    local include_canary="$1" include_rest="$2" i
    local -n out_ref=$3
    out_ref=()
    for ((i = 0; i < ${#H_NAME[@]}; i++)); do
        should_deploy "${H_NAME[$i]}" || continue
        if [[ "${H_CANARY[$i]}" == true ]]; then
            [[ "$include_canary" == true ]] || continue
        else
            [[ "$include_rest" == true ]] || continue
        fi
        out_ref+=("$i")
    done
}

host_exec() {
    local is_local="$1" ssh_target="$2" cmd="$3"
    if [[ "$DRY_RUN" == true ]]; then
        local where="ssh $ssh_target"
        [[ "$is_local" == true ]] && where=local
        echo -e "${YELLOW}  [dry-run]${NC} $where :: $cmd"
        return 0
    fi
    if [[ "$is_local" == true ]]; then
        bash -lc "$cmd"
    else
        ssh "$ssh_target" "bash -lc $(printf '%q' "$cmd")"
    fi
}

host_exec_sudo() {
    host_exec "$1" "$2" "sudo -n bash -c $(printf '%q' "$3")"
}

# --- release artifact -------------------------------------------------------

declared_version() {
    python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1],"rb"))["workspace"]["package"]["version"])' \
        "$ROOT_DIR/backend/Cargo.toml"
}

require_clean_source() {
    local dirty
    dirty="$(git -C "$ROOT_DIR" status --porcelain --untracked-files=normal)" \
        || fail "cannot read git status of $ROOT_DIR"
    if [[ -n "$dirty" ]]; then
        err "Refusing to build a deployment from a worktree with uncommitted or untracked changes:"
        printf '%s\n' "$dirty" | head -20 >&2
        fail "commit, stash or deploy a prebuilt --artifact instead"
    fi
}

build_artifact() {
    local version target
    version="$(declared_version)"
    target="$(uname -m)-unknown-linux-gnu"
    require_clean_source
    python3 "$ROOT_DIR/scripts/release/check-tag-version.py" --tag "v$version" \
        || fail "Cargo, dependency pin and frontend versions disagree; use 'just bump'"
    DEPLOY_ARTIFACT="$ROOT_DIR/dist/out/oqto-${version}-${target}.tar.gz"
    DEPLOY_CHECKSUM="${DEPLOY_ARTIFACT}.sha256"
    log "Building release $version ($target) from $(git -C "$ROOT_DIR" rev-parse --short HEAD)"
    if [[ "$DRY_RUN" == true ]]; then
        echo -e "${YELLOW}  [dry-run]${NC} just dist-sync / dist-stage-pi-runtime / dist-stage-binaries --build / dist-stage-frontend --build / lint-dist-manifest-strict / dist-package $version $target"
        return 0
    fi
    (
        cd "$ROOT_DIR"
        # Same sequence as .github/workflows/release.yml (checked by
        # scripts/tests/deploy-mode-test.sh): the bundled Pi runtime is part
        # of every full release.
        just dist-sync
        just dist-stage-pi-runtime
        DIST_REQUIRE_GLIBC_FLOOR="${DIST_REQUIRE_GLIBC_FLOOR:-1}" just dist-stage-binaries --build
        just dist-stage-frontend --build
        just lint-dist-manifest-strict
        just dist-package "$version" "$target"
    ) || fail "release build failed"
    # The build itself must not dirty the tree (it would make provenance lie).
    require_clean_source
    [[ -f "$DEPLOY_ARTIFACT" && -f "$DEPLOY_CHECKSUM" ]] || fail "dist-package did not produce $DEPLOY_ARTIFACT"
    ok "Built $DEPLOY_ARTIFACT"
}

# Stage files in a per-host private directory; never predictable /tmp names.
stage_private_remote_release() {
    local ssh_target="$1" remote_dir artifact_source checksum_source
    local archive_name="${DEPLOY_ARTIFACT##*/}" checksum_name="${DEPLOY_CHECKSUM##*/}"
    if [[ ! "$archive_name" =~ ^oqto-v?[0-9][a-zA-Z0-9._+-]*-(x86_64|aarch64)-unknown-linux-gnu\.tar\.gz$ \
        || ! "$checksum_name" =~ ^[a-zA-Z0-9._+-]+$ ]]; then
        err "unsafe release or checksum basename for remote staging"
        return 1
    fi
    artifact_source="$(realpath -e -- "$DEPLOY_ARTIFACT")" || return 1
    checksum_source="$(realpath -e -- "$DEPLOY_CHECKSUM")" || return 1
    if [[ "${artifact_source##*/}" != "$archive_name" || "${checksum_source##*/}" != "$checksum_name" \
        || "$artifact_source" == *:* || "$checksum_source" == *:* ]]; then
        err "remote staging refuses ambiguous source paths"
        return 1
    fi
    remote_dir="$(ssh "$ssh_target" 'umask 077; mktemp -d /tmp/oqto-deploy.XXXXXXXX')" || return 1
    if [[ ! "$remote_dir" =~ ^/tmp/oqto-deploy\.[a-zA-Z0-9]{8}$ ]]; then
        err "remote staging did not return a private canonical directory"
        return 1
    fi
    if ! scp -- "$artifact_source" "$checksum_source" "$TRANSACTION" "$BOOTSTRAP" \
        "$ssh_target:$remote_dir/" >/dev/null; then
        ssh "$ssh_target" "rm -rf -- $(printf '%q' "$remote_dir")" >/dev/null 2>&1 || true
        err "failed to stage release files privately"
        return 1
    fi
    printf '%s\n' "$remote_dir"
}

cleanup_private_remote_release() {
    local ssh_target="$1" remote_dir="$2"
    [[ "$remote_dir" =~ ^/tmp/oqto-deploy\.[a-zA-Z0-9]{8}$ ]] || return 1
    ssh "$ssh_target" "rm -rf -- $(printf '%q' "$remote_dir")"
}

# Paths of the artifact, checksum, transaction and bootstrap as seen on the
# target. Sets STAGED_DIR (remote private dir or empty) and TARGET_* vars.
stage_for_host() {
    local is_local="$1" ssh_target="$2"
    STAGED_DIR=""
    if [[ "$is_local" == true ]]; then
        TARGET_ARTIFACT="$DEPLOY_ARTIFACT"; TARGET_CHECKSUM="$DEPLOY_CHECKSUM"
        TARGET_TRANSACTION="$TRANSACTION"; TARGET_BOOTSTRAP="$BOOTSTRAP"
        return 0
    fi
    if [[ "$DRY_RUN" == true ]]; then
        STAGED_DIR="/tmp/oqto-deploy.DRYRUN00"
    else
        STAGED_DIR="$(stage_private_remote_release "$ssh_target")" || return 1
    fi
    TARGET_ARTIFACT="$STAGED_DIR/${DEPLOY_ARTIFACT##*/}"
    TARGET_CHECKSUM="$STAGED_DIR/${DEPLOY_CHECKSUM##*/}"
    TARGET_TRANSACTION="$STAGED_DIR/${TRANSACTION##*/}"
    TARGET_BOOTSTRAP="$STAGED_DIR/${BOOTSTRAP##*/}"
}

# --- dependency policy -------------------------------------------------------

declare -a REQUIRED_DEP_BINARIES=()
declare -A REQUIRED_DEP_VERSIONS=()

load_dependency_requirements() {
    [[ -f "$DEPENDENCY_POLICY_FILE" ]] || fail "Dependency policy file missing: $DEPENDENCY_POLICY_FILE"
    local bin ver
    while IFS='=' read -r bin ver; do
        [[ -z "$bin" || -z "$ver" ]] && continue
        REQUIRED_DEP_BINARIES+=("$bin")
        REQUIRED_DEP_VERSIONS["$bin"]="$ver"
    done < <(python3 - "$DEPENDENCY_POLICY_FILE" <<'PY'
import sys, tomllib
byteowlz = tomllib.load(open(sys.argv[1], "rb")).get("byteowlz", {})
# Deploy-critical CLIs; hstry is legacy interop and never gated (ADR-0005).
for key in ("eavs", "mmry", "trx", "agntz", "sx", "skdlr"):
    value = str(byteowlz.get(key, "")).strip()
    if value and value != "latest":
        print(f"{key}={value}")
PY
)
}

version_ge() {
    [[ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" == "$2" ]]
}

installed_version() {
    local is_local="$1" ssh_target="$2" dep="$3"
    host_exec "$is_local" "$ssh_target" \
        "{ /usr/local/bin/$dep --version 2>/dev/null || $dep --version 2>/dev/null; } | grep -oE '[0-9]+\\.[0-9]+\\.[0-9]+' | head -1" \
        2>/dev/null || true
}

# Release mode: tools older than their pin are remediated through the
# artifact's own oqto-setup (checksum-verified prebuilt acquisition). Newer
# installed tools are always preserved. Dev mode only requires presence.
check_dependency_compatibility() {
    local name="$1" ssh_target="$2" is_local="$3" dep current
    local -a outdated=()
    [[ "${#REQUIRED_DEP_BINARIES[@]}" -eq 0 ]] && return 0
    if [[ "$DEPLOY_MODE" == dev ]]; then
        log "  Development dependency mode: preserving target-installed tools"
    fi
    for dep in "${REQUIRED_DEP_BINARIES[@]}"; do
        if [[ "$DRY_RUN" == true ]]; then
            if [[ "$DEPLOY_MODE" == dev ]]; then
                echo -e "${YELLOW}  [dry-run]${NC} dependency presence gate: $dep (installed version preserved)"
            else
                echo -e "${YELLOW}  [dry-run]${NC} dependency gate: $dep >= ${REQUIRED_DEP_VERSIONS[$dep]}"
            fi
            continue
        fi
        current="$(installed_version "$is_local" "$ssh_target" "$dep")"
        if [[ "$DEPLOY_MODE" == dev ]]; then
            if ! host_exec "$is_local" "$ssh_target" "test -x '/usr/local/bin/$dep' || command -v '$dep' >/dev/null" 2>/dev/null; then
                err "  $dep is required but not installed on $name (dev mode never installs tools)"
                return 1
            fi
            log "  $dep: ${current:-unknown version} (preserved)"
        elif [[ -z "$current" ]] || ! version_ge "$current" "${REQUIRED_DEP_VERSIONS[$dep]}"; then
            warn "  $dep: ${current:-missing} (need >= ${REQUIRED_DEP_VERSIONS[$dep]})"
            outdated+=("$dep")
        else
            log "  $dep: $current (ok)"
        fi
    done
    [[ "${#outdated[@]}" -eq 0 ]] && return 0
    err "  Outdated tools on $name: ${outdated[*]}"
    err "  Acquire them first with the release's checksum-verified tool path:"
    err "    oqto-setup acquire --manifest dependencies.toml --install-bin /usr/local/bin --tools-only"
    err "  (or deploy with --mode dev to preserve the installed versions)."
    return 1
}

# --- preflight ---------------------------------------------------------------

preflight_host() {
    local i="$1" name="${H_NAME[$1]}" ssh_target="${H_SSH[$1]}" is_local="${H_LOCAL[$1]}"
    local profile
    profile="$(profile_for "${H_MODE[$i]}")"
    [[ "$profile" != invalid ]] || { err "Host $name has invalid mode '${H_MODE[$i]}'"; return 1; }
    if [[ "$profile" == personal && ( -z "${H_USER[$i]}" || "${H_USER[$i]}" == root ) ]]; then
        err "Host $name: personal hosts need a non-root 'user' (the owning principal)"
        return 1
    fi
    if [[ "${H_FRONTEND[$i]}" == true && -n "${H_WEB_ROOT[$i]}" && "${H_WEB_ROOT[$i]}" != "$CANONICAL_WEB_ROOT" ]]; then
        err "Host $name: web_root must be $CANONICAL_WEB_ROOT (oqto-setup links the release frontend there)"
        return 1
    fi
    if [[ "$is_local" != true && "$DRY_RUN" != true ]]; then
        ssh -o ConnectTimeout=5 "$ssh_target" true >/dev/null 2>&1 || { err "Cannot reach $ssh_target"; return 1; }
    fi
    [[ "$BACKFILL_ONLY" == true || "$STATUS_ONLY" == true ]] && return 0

    host_exec_sudo "$is_local" "$ssh_target" \
        "free=\$(df -Pm /var/lib 2>/dev/null | awk 'NR==2{print \$4}'); [[ \${free:-0} -ge $MIN_FREE_MB ]]" \
        || { err "Less than ${MIN_FREE_MB}MB free under /var/lib on $name"; return 1; }
    if [[ "$profile" == team ]]; then
        host_exec_sudo "$is_local" "$ssh_target" \
            "[[ \"\$(stat -c '%U:%G' /etc/oqto/sandbox.toml)\" == root:root && \$(stat -c '%a' /etc/oqto/sandbox.toml) -le 644 ]]" \
            || { err "/etc/oqto/sandbox.toml on $name must exist, be root:root and not group/other-writable"; return 1; }
        # Release activation does not install policy assets; an enforcing
        # sandbox must already have its BPF or every runner spawn would fail.
        host_exec_sudo "$is_local" "$ssh_target" \
            "! grep -Eq '^[[:space:]]*(seccomp_enforce[[:space:]]*=[[:space:]]*true|seccomp(_mode)?[[:space:]]*=[[:space:]]*\"enforce\")' /etc/oqto/sandbox.toml || test -s /usr/local/share/oqto/seccomp/default.bpf" \
            || { err "seccomp enforcement is configured on $name but /usr/local/share/oqto/seccomp/default.bpf is missing"; return 1; }
    fi
    check_dependency_compatibility "$name" "$ssh_target" "$is_local" || return 1
    ok "Preflight passed on $name"
}

# --- target transactions -----------------------------------------------------

transaction_args() {
    local i="$1" profile
    profile="$(profile_for "${H_MODE[$i]}")"
    TX_ARGS=(--profile "$profile")
    [[ "$profile" == personal ]] && TX_ARGS+=(--principal "${H_USER[$i]}")
    [[ "$ALLOW_BUSY_SESSIONS" == true ]] && TX_ARGS+=(--allow-busy-sessions)
    return 0
}

run_transaction() {
    local i="$1" is_local="${H_LOCAL[$1]}" ssh_target="${H_SSH[$1]}" verb="$2"
    local status=0 script
    stage_for_host "$is_local" "$ssh_target" || return 1
    transaction_args "$i"
    local -a args=("${TX_ARGS[@]}")
    if [[ "$verb" == activate ]]; then
        args+=(--artifact "$TARGET_ARTIFACT" --checksum "$TARGET_CHECKSUM" --setup "@SETUP@"
            --health-budget "$HEALTH_TIMEOUT_SECONDS" --keep-releases "$KEEP_RELEASES")
        [[ "$ALLOW_DOWNGRADE" == true ]] && args+=(--allow-downgrade)
        [[ "$ALLOW_DIRTY_SOURCE" == true ]] && args+=(--allow-dirty-source)
    else
        args+=(--modes "$BACKFILL_MODES")
        local principal
        for principal in "${BACKFILL_PRINCIPALS[@]}"; do args+=(--only "$principal"); done
    fi
    local quoted
    printf -v quoted '%q ' "${args[@]}"
    # The installer comes only from the verified artifact, never from PATH.
    script="set -euo pipefail
umask 077
tmp=\$(mktemp -d)
trap 'rm -rf -- \"\$tmp\"' EXIT
transaction=$(printf '%q' "$TARGET_TRANSACTION")"
    if [[ "$verb" == activate ]]; then
        script+="
python3 $(printf '%q' "$TARGET_BOOTSTRAP") --artifact $(printf '%q' "$TARGET_ARTIFACT") \\
    --checksum $(printf '%q' "$TARGET_CHECKSUM") --output \"\$tmp/oqto-setup\"
args=(${quoted//@SETUP@/\"\$tmp/oqto-setup\"})"
    else
        script+="
args=($quoted)"
    fi
    script+="
python3 \"\$transaction\" $verb \"\${args[@]}\""
    host_exec_sudo "$is_local" "$ssh_target" "$script" || status=$?
    if [[ -n "$STAGED_DIR" && "$DRY_RUN" != true ]]; then
        cleanup_private_remote_release "$ssh_target" "$STAGED_DIR" || status=1
    fi
    return "$status"
}

sync_extensions_host() {
    local i="$1" is_local="${H_LOCAL[$1]}" ssh_target="${H_SSH[$1]}"
    local sync="$ROOT_DIR/scripts/dist/sync-agent-runtime.sh" list="$ROOT_DIR/scripts/dist/pi-default-extensions.txt"
    [[ "$SYNC_EXTENSIONS" == true ]] || return 0
    if [[ "$DRY_RUN" == true ]]; then
        echo -e "${YELLOW}  [dry-run]${NC} sync pinned Pi extensions (preserve-first, owner-correct) on ${H_NAME[$i]}"
        return 0
    fi
    log "[${H_NAME[$i]}] syncing pinned Pi extensions (runtime comes from the release)"
    if [[ "$is_local" == true ]]; then
        bash "$sync" --manifest "$DEPENDENCY_POLICY_FILE" --skip-pi
        return
    fi
    local dir
    dir="$(ssh "$ssh_target" 'umask 077; mktemp -d /tmp/oqto-extensions.XXXXXXXX')" || return 1
    [[ "$dir" =~ ^/tmp/oqto-extensions\.[a-zA-Z0-9]{8}$ ]] || return 1
    scp -- "$sync" "$list" "$DEPENDENCY_POLICY_FILE" "$ssh_target:$dir/" >/dev/null || return 1
    local q
    q="$(printf '%q' "$dir")"
    ssh "$ssh_target" "PI_DEFAULT_EXTENSIONS_FILE=$q/pi-default-extensions.txt bash $q/sync-agent-runtime.sh --manifest $q/dependencies.toml --skip-pi; rc=\$?; rm -rf -- $q; exit \$rc"
}

print_status_host() {
    local i="$1"
    host_exec_sudo "${H_LOCAL[$i]}" "${H_SSH[$i]}" \
        "for link in current last-good; do printf '%s=%s\n' \"\$link\" \"\$(basename \"\$(readlink -f /var/lib/oqto/releases/\$link 2>/dev/null || echo none)\")\"; done"
}

deploy_host() {
    local i="$1" name="${H_NAME[$1]}"
    echo ""
    log "=========================================="
    log "${BOLD}$name${NC} ($(profile_for "${H_MODE[$i]}"))"
    log "=========================================="
    prime_sudo "$i" || return 1
    preflight_host "$i" || return 1
    if [[ "$STATUS_ONLY" == true ]]; then
        print_status_host "$i"
        return
    fi
    if [[ "$BACKFILL_ONLY" != true ]]; then
        run_transaction "$i" activate || { err "[$name] activation failed; oqto-setup kept the previous release"; return 1; }
        ok "[$name] activated ${DEPLOY_ARTIFACT##*/}"
        sync_extensions_host "$i" || { err "[$name] extension sync failed (release stays active)"; return 1; }
    fi
    if [[ "$BACKFILL" == true || "$BACKFILL_ONLY" == true ]]; then
        run_transaction "$i" backfill || { err "[$name] backfill failed; affected stores were restored from snapshots"; return 1; }
        ok "[$name] history backfill complete"
    fi
}

run_targets() {
    local -n target_ref=$1
    local i
    for i in "${target_ref[@]}"; do
        deploy_host "$i" || { err "Deployment stopped at ${H_NAME[$i]}"; return 1; }
    done
}

# Validated per host right before its transaction, so a long earlier host can
# never leave a later one running with an expired credential mid-way.
prime_sudo() {
    local i="$1"
    [[ "$DRY_RUN" == true ]] && return 0
    if [[ "${H_LOCAL[$i]}" == true ]]; then
        sudo -v || { err "sudo authentication failed on ${H_NAME[$i]}"; return 1; }
    elif ! ssh "${H_SSH[$i]}" "sudo -n true" >/dev/null 2>&1; then
        ssh -tt "${H_SSH[$i]}" "sudo -v" </dev/tty || { err "sudo authentication failed on ${H_NAME[$i]}"; return 1; }
    fi
}

# --- main ----------------------------------------------------------------------

parse_hosts
load_dependency_requirements
python3 "$ROOT_DIR/scripts/dist/verify-immutable-migrations.py"

declare -a selected=()
if [[ "$CANARY_ONLY" == true ]]; then
    collect_targets true false selected
else
    collect_targets true true selected
fi
[[ "${#selected[@]}" -gt 0 ]] || fail "No hosts matched filters"

if [[ "$STATUS_ONLY" != true && "$BACKFILL_ONLY" != true && -z "$DEPLOY_ARTIFACT" ]]; then
    build_artifact
fi

if [[ "$CANARY_THEN_FLEET" == true ]]; then
    declare -a canary=() fleet=()
    collect_targets true false canary
    collect_targets false true fleet
    [[ "${#canary[@]}" -gt 0 ]] || fail "No canary hosts selected"
    run_targets canary
    ok "Canary deployment passed"
    [[ "${#fleet[@]}" -eq 0 ]] || run_targets fleet
else
    run_targets selected
fi

ok "Done"
