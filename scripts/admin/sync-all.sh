#!/usr/bin/env bash
# sync-all.sh - Full sync: eavs + pi config + skills for all users
#
# Runs all provisioning steps in order:
#   1. Sync onboarding templates from remote
#   2. Provision/verify EAVS keys for all users
#   3. Sync Pi configuration (models.json, settings, AGENTS.md)
#   4. Update skills for all users
#
# This is the "make everything right" command for when things get out of sync.
#
# Usage:
#   sync-all.sh                    Sync everything for all active users
#   sync-all.sh --user alice       Sync everything for alice only
#   sync-all.sh --skip-eavs        Skip EAVS provisioning
#   sync-all.sh --skip-skills      Skip skills sync
#   sync-all.sh --dry-run          Show what would be done

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

TARGET_USER=""
TARGET_ALL=true  # Default to all
DRY_RUN=false
SKIP_EAVS=false
SKIP_SKILLS=false
SKIP_TEMPLATES=false
FORCE=false

usage() {
    cat <<EOF
${BOLD}sync-all${RESET} - Full provisioning sync for all users

${BOLD}USAGE${RESET}
    sync-all.sh [options]

${BOLD}OPTIONS${RESET}
    --user, -u <name>   Sync for a specific user only
    --skip-eavs         Skip EAVS key provisioning
    --skip-skills       Skip skills update
    --skip-templates    Skip template sync
    --force             Overwrite existing user configs
    --dry-run           Show what would be done
    -h, --help          Show this help

${BOLD}STEPS${RESET}
    1. Sync onboarding templates from remote repo
    2. Verify/provision EAVS keys (models.json + eavs.env)
    3. Sync Pi configuration (settings.json, AGENTS.md)
    4. Update installed skills from source

EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user|-u)        TARGET_USER="$2"; TARGET_ALL=false; shift 2 ;;
        --skip-eavs)      SKIP_EAVS=true; shift ;;
        --skip-skills)    SKIP_SKILLS=true; shift ;;
        --skip-templates) SKIP_TEMPLATES=true; shift ;;
        --force)          FORCE=true; shift ;;
        --dry-run)        DRY_RUN=true; shift ;;
        -h|--help)        usage ;;
        *)                log_error "Unknown option: $1"; usage ;;
    esac
done

# Build target args
target_args=()
if [[ -n "$TARGET_USER" ]]; then
    target_args+=(--user "$TARGET_USER")
else
    target_args+=(--all)
fi

extra_args=()
$DRY_RUN && extra_args+=(--dry-run)
$FORCE && extra_args+=(--force)

errors=0

echo ""
echo -e "${BOLD}=== Oqto Full Sync ===${RESET}"
echo ""

# Step 1: Sync templates
if ! $SKIP_TEMPLATES; then
    echo -e "${BOLD}[1/4] Syncing templates...${RESET}"
    "$SCRIPT_DIR/manage-templates.sh" --sync "${extra_args[@]}" || {
        log_warn "Template sync had errors (continuing)"
        ((errors++)) || true
    }
    echo ""
else
    echo -e "${DIM}[1/4] Templates sync skipped${RESET}"
fi

# Step 2: EAVS provisioning
if ! $SKIP_EAVS; then
    echo -e "${BOLD}[2/4] Provisioning EAVS keys...${RESET}"
    "$SCRIPT_DIR/eavs-provision.sh" --sync-models "${target_args[@]}" "${extra_args[@]}" || {
        log_warn "EAVS sync had errors (continuing)"
        ((errors++)) || true
    }
    echo ""
else
    echo -e "${DIM}[2/4] EAVS provisioning skipped${RESET}"
fi

# Step 3: Pi config sync
echo -e "${BOLD}[3/4] Syncing Pi configuration...${RESET}"
"$SCRIPT_DIR/sync-pi-config.sh" "${target_args[@]}" "${extra_args[@]}" || {
    log_warn "Pi config sync had errors (continuing)"
    ((errors++)) || true
}
echo ""

# Step 4: Skills update
if ! $SKIP_SKILLS; then
    echo -e "${BOLD}[4/4] Updating skills...${RESET}"
    "$SCRIPT_DIR/manage-skills.sh" --update "${target_args[@]}" "${extra_args[@]}" || {
        log_warn "Skills update had errors (continuing)"
        ((errors++)) || true
    }
    echo ""
else
    echo -e "${DIM}[4/4] Skills update skipped${RESET}"
fi

# Summary
echo -e "${BOLD}=== Sync Complete ===${RESET}"
if [[ $errors -eq 0 ]]; then
    log_ok "All steps completed successfully"
else
    log_warn "$errors step(s) had errors"
fi
