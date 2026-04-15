#!/usr/bin/env bash
set -euo pipefail

# Sandbox profile matrix checks:
# 1) CLI-path smoke for all built-in profiles (minimal/development/strict)
# 2) Runner-path + Pi + EAVS mock streaming checks for network-enabled profiles
#    (minimal/development) via scripts/e2e-streaming-test.sh

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
E2E_SCRIPT="$ROOT_DIR/scripts/e2e-streaming-test.sh"
SANDBOX_CFG_DIR="${HOME}/.config/oqto"
SANDBOX_CFG="${SANDBOX_CFG_DIR}/sandbox.toml"
BACKUP_CFG=""
HAD_ORIGINAL_CFG=0

OQTO_URL="${OQTO_URL:-http://localhost:8080}"
TIMEOUT="${TIMEOUT:-30}"
TEST_USER="${OQTO_TEST_USER:-dev}"
TEST_PASS="${OQTO_TEST_PASSWORD:-dev}"
RESTART_SERVICES="${RESTART_SERVICES:-1}"
RUNNER_E2E="${RUNNER_E2E:-1}"
ARTIFACT_DIR="${OQTO_TEST_ARTIFACT_DIR:-$ROOT_DIR/scripts/e2e/logs}"
SANDBOX_BIN="${SANDBOX_BIN:-$ROOT_DIR/backend/target/release/oqto-sandbox}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[sandbox-matrix]${NC} $*"; }
ok() { echo -e "${GREEN}[PASS]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
fail() { echo -e "${RED}[FAIL]${NC} $*"; }

restore_config() {
    if [[ -n "$BACKUP_CFG" && -f "$BACKUP_CFG" ]]; then
        mkdir -p "$SANDBOX_CFG_DIR"
        cp "$BACKUP_CFG" "$SANDBOX_CFG"
        rm -f "$BACKUP_CFG"
        log "Restored original sandbox config: $SANDBOX_CFG"
    elif [[ "$HAD_ORIGINAL_CFG" -eq 0 ]]; then
        rm -f "$SANDBOX_CFG"
    fi
}

cleanup() {
    restore_config || true
}
trap cleanup EXIT

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        fail "Missing required command: $1"
        exit 1
    fi
}

write_profile_config() {
    local profile="$1"
    mkdir -p "$SANDBOX_CFG_DIR"
    cat > "$SANDBOX_CFG" <<EOF
enabled = true
profile = "$profile"
EOF
    log "Wrote sandbox profile '$profile' to $SANDBOX_CFG"
}

restart_runtime() {
    if [[ "$RESTART_SERVICES" != "1" ]]; then
        warn "Skipping service restart (RESTART_SERVICES=$RESTART_SERVICES)"
        return 0
    fi

    if systemctl --user list-unit-files 2>/dev/null | grep -q '^oqto-runner\.service'; then
        log "Restarting oqto-runner"
        systemctl --user restart oqto-runner
    else
        warn "oqto-runner.service not found in user systemd; skipping runner restart"
    fi

    if systemctl --user list-unit-files 2>/dev/null | grep -q '^oqto\.service'; then
        log "Restarting oqto backend"
        systemctl --user restart oqto
    else
        warn "oqto.service not found in user systemd; skipping backend restart"
    fi

    sleep 2
}

write_profile_fixture() {
    local profile="$1"
    local cfg="$ARTIFACT_DIR/sandbox-${profile}.toml"
    cat > "$cfg" <<EOF
enabled = true
profile = "$profile"
EOF
    echo "$cfg"
}

cli_smoke() {
    local profile="$1"
    local cfg
    cfg="$(write_profile_fixture "$profile")"

    log "CLI smoke: profile=$profile"
    "$SANDBOX_BIN" --config "$cfg" --workspace "$ROOT_DIR" -- echo "cli-ok-$profile" >"$ARTIFACT_DIR/oqto-sandbox-cli-${profile}.log" 2>&1
    if ! grep -q "cli-ok-$profile" "$ARTIFACT_DIR/oqto-sandbox-cli-${profile}.log"; then
        fail "CLI echo smoke failed for profile=$profile"
        cat "$ARTIFACT_DIR/oqto-sandbox-cli-${profile}.log"
        exit 1
    fi

    "$SANDBOX_BIN" --config "$cfg" --workspace "$ROOT_DIR" -- pi --version >"$ARTIFACT_DIR/oqto-sandbox-pi-${profile}.log" 2>&1
    ok "CLI smoke passed for profile=$profile"
}

strict_eavs_expected_fail_smoke() {
    local cfg
    cfg="$(write_profile_fixture "strict")"

    log "Strict profile expected-fail smoke: localhost:3033 should be unreachable"
    set +e
    "$SANDBOX_BIN" --config "$cfg" --workspace "$ROOT_DIR" -- curl -sSf http://localhost:3033/providers >"$ARTIFACT_DIR/oqto-sandbox-strict-eavs.log" 2>&1
    local rc=$?
    set -e

    if [[ "$rc" -eq 0 ]]; then
        fail "strict profile unexpectedly reached localhost:3033"
        cat "$ARTIFACT_DIR/oqto-sandbox-strict-eavs.log"
        exit 1
    fi

    ok "strict profile blocks localhost:3033 as expected (rc=$rc)"
}

runner_stream_smoke() {
    local profile="$1"
    log "Runner + Pi + EAVS mock smoke: profile=$profile"

    write_profile_config "$profile"
    restart_runtime

    OQTO_URL="$OQTO_URL" \
    OQTO_TEST_USER="$TEST_USER" \
    OQTO_TEST_PASSWORD="$TEST_PASS" \
    "$E2E_SCRIPT" --scenario simple_text --timeout "$TIMEOUT" >"$ARTIFACT_DIR/oqto-e2e-${profile}.log" 2>&1

    ok "Runner streaming smoke passed for profile=$profile"
}

main() {
    if [[ ! -x "$SANDBOX_BIN" ]]; then
        require_cmd oqto-sandbox
        SANDBOX_BIN="oqto-sandbox"
    fi
    require_cmd pi
    require_cmd curl
    require_cmd jq

    if [[ ! -x "$E2E_SCRIPT" ]]; then
        fail "Missing executable e2e script: $E2E_SCRIPT"
        exit 1
    fi

    mkdir -p "$ARTIFACT_DIR"

    if [[ -f "$SANDBOX_CFG" ]]; then
        HAD_ORIGINAL_CFG=1
        BACKUP_CFG="$(mktemp "${SANDBOX_CFG_DIR}/sandbox.backup.XXXXXX.toml")"
        cp "$SANDBOX_CFG" "$BACKUP_CFG"
        log "Backed up existing sandbox config to $BACKUP_CFG"
    fi

    log "=== Phase 1: CLI path smoke (all built-in profiles) ==="
    for profile in minimal development strict; do
        cli_smoke "$profile"
    done
    strict_eavs_expected_fail_smoke

    if [[ "$RUNNER_E2E" == "1" ]]; then
        log "=== Phase 2: runner path + EAVS mock smoke (network-enabled profiles) ==="
        require_cmd websocat

        for profile in minimal development; do
            if ! runner_stream_smoke "$profile"; then
                fail "Runner streaming smoke failed for profile=$profile"
                cat "$ARTIFACT_DIR/oqto-e2e-${profile}.log" || true
                exit 1
            fi
        done
    else
        warn "Skipping runner-path E2E phase (RUNNER_E2E=$RUNNER_E2E)"
    fi

    warn "Strict profile is validated via CLI Pi startup only."
    warn "Strict profile intentionally isolates network; model streaming via EAVS is expected to fail there."

    ok "Sandbox profile matrix completed"
    echo "Artifacts:"
    echo "  $ARTIFACT_DIR/oqto-sandbox-cli-<profile>.log"
    echo "  $ARTIFACT_DIR/oqto-sandbox-pi-<profile>.log"
    echo "  $ARTIFACT_DIR/oqto-e2e-<profile>.log"
}

main "$@"
