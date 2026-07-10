# ==============================================================================
# Pi Extensions Installation
# ==============================================================================
#
# Extensions are synced by the shared scripts/dist/sync-agent-runtime.sh — the
# SAME script `deploy` runs — to the pinned pi-agent-extensions ref from
# dependencies.toml. It installs them system-wide (/usr/share/oqto/pi-agent-
# extensions, the source oqto-usermgr copies for NEW users) and into every
# existing platform user's ~/.pi/agent/extensions + /etc/skel.
#
# Both entry points below (dispatched by 21-main.sh for single vs multi-user)
# delegate to it, so setup and deploy can't drift. --skip-pi runs extensions
# only (pi is handled by ensure_bun_and_pi_global in 05-install-core.sh).

install_pi_extensions() {
  log_step "Installing Pi extensions"
  "${SCRIPT_DIR}/scripts/dist/sync-agent-runtime.sh" --skip-pi
}

install_pi_extensions_all_users() {
  log_step "Installing Pi extensions for all users"
  "${SCRIPT_DIR}/scripts/dist/sync-agent-runtime.sh" --skip-pi
}
