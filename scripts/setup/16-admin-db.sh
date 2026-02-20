# ==============================================================================
# Admin User Database Setup
# ==============================================================================

create_admin_user_db() {
  if [[ "$PRODUCTION_MODE" != "true" ]]; then
    return 0
  fi

  log_step "Creating admin user"

  local admin_user="${ADMIN_USERNAME:-}"
  local admin_email="${ADMIN_EMAIL:-}"

  # Load from creds file if available
  local creds_file="$OQTO_CONFIG_DIR/.admin_setup"
  if [[ -f "$creds_file" ]]; then
    # shellcheck source=/dev/null
    source "$creds_file"
    admin_user="${ADMIN_USERNAME:-$admin_user}"
    admin_email="${ADMIN_EMAIL:-$admin_email}"
  fi

  if [[ -z "$admin_user" || -z "$admin_email" ]]; then
    log_warn "Admin username/email not set. Re-run setup or create manually:"
    log_info "  oqtoctl user bootstrap --username <user> --email <email>"
    return 0
  fi

  # Find oqtoctl
  local oqtoctl_bin=""
  if [[ -x "${TOOLS_INSTALL_DIR}/oqtoctl" ]]; then
    oqtoctl_bin="${TOOLS_INSTALL_DIR}/oqtoctl"
  elif command_exists oqtoctl; then
    oqtoctl_bin="oqtoctl"
  else
    log_error "oqtoctl not found. Run the build step first."
    return 1
  fi

  # Ensure database exists by starting the service (runs migrations)
  local db_path=""
  if [[ "${SELECTED_USER_MODE:-}" == "multi" ]]; then
    db_path="/var/lib/oqto/.local/share/oqto/oqto.db"
    # Migrate from old name
    local old_db="/var/lib/oqto/.local/share/oqto/sessions.db"
    if [[ -f "$old_db" && ! -f "$db_path" ]]; then
      sudo mv "$old_db" "$db_path"
      sudo mv "${old_db}-wal" "${db_path}-wal" 2>/dev/null || true
      sudo mv "${old_db}-shm" "${db_path}-shm" 2>/dev/null || true
    fi
    sudo mkdir -p "$(dirname "$db_path")"
    sudo chown -R oqto:oqto /var/lib/oqto/.local
  else
    local data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
    db_path="${data_dir}/oqto/oqto.db"
    mkdir -p "$(dirname "$db_path")"
  fi

  if [[ ! -f "$db_path" ]]; then
    log_info "Starting service to initialize database..."
    if [[ "${SELECTED_USER_MODE:-}" == "multi" ]]; then
      sudo systemctl start oqto 2>/dev/null || true
    else
      systemctl --user start oqto 2>/dev/null || true
    fi
    # Wait for DB to appear
    local retries=0
    while [[ ! -f "$db_path" && $retries -lt 15 ]]; do
      sleep 1
      retries=$((retries + 1))
    done
    # Stop the service again so bootstrap can write to the DB
    if [[ "${SELECTED_USER_MODE:-}" == "multi" ]]; then
      sudo systemctl stop oqto 2>/dev/null || true
    else
      systemctl --user stop oqto 2>/dev/null || true
    fi
  fi

  if [[ ! -f "$db_path" ]]; then
    log_warn "Database not found at $db_path"
    log_info "Create admin user manually after starting Oqto:"
    log_info "  $oqtoctl_bin user bootstrap --username \"$admin_user\" --email \"$admin_email\""
    return 0
  fi

  # Check if user already exists
  if command_exists sqlite3; then
    local existing
    if [[ "${SELECTED_USER_MODE:-}" == "multi" ]]; then
      existing=$(sudo sqlite3 "$db_path" "SELECT COUNT(*) FROM users WHERE username = '$admin_user';" 2>/dev/null || echo "0")
    else
      existing=$(sqlite3 "$db_path" "SELECT COUNT(*) FROM users WHERE username = '$admin_user';" 2>/dev/null || echo "0")
    fi
    if [[ "$existing" -gt 0 ]]; then
      log_info "Admin user '$admin_user' already exists, skipping"
      rm -f "$creds_file"
      return 0
    fi
  fi

  # Hash the password first (runs as current user -- no DB access needed)
  local admin_hash=""
  if [[ "$NONINTERACTIVE" == "true" ]]; then
    local admin_password
    admin_password=$(generate_secure_secret 16)
    admin_hash=$("$oqtoctl_bin" hash-password --password "$admin_password")
    log_info "Generated admin password: $admin_password"
    log_warn "SAVE THIS PASSWORD - it will not be shown again!"
  else
    log_info "Set the admin password:"
    admin_hash=$("$oqtoctl_bin" hash-password)
  fi

  if [[ -z "$admin_hash" ]]; then
    log_error "Failed to hash password"
    return 1
  fi

  # Build bootstrap args with pre-computed hash (no interactive prompts needed)
  local bootstrap_args=(user bootstrap --username "$admin_user" --email "$admin_email")
  bootstrap_args+=(--database "$db_path")
  bootstrap_args+=(--password-hash "$admin_hash")
  # Skip Linux user creation during setup -- it happens at first login via oqto-usermgr
  bootstrap_args+=(--no-linux-user)

  # In multi-user mode, run as oqto user (DB is owned by oqto)
  local run_prefix=()
  if [[ "${SELECTED_USER_MODE:-}" == "multi" ]]; then
    run_prefix=(sudo -u oqto)
  fi

  if "${run_prefix[@]}" "$oqtoctl_bin" "${bootstrap_args[@]}"; then
    log_success "Admin user '$admin_user' created"
  else
    log_warn "Failed to create admin user. Create manually:"
    log_info "  sudo -u oqto $oqtoctl_bin user bootstrap --username \"$admin_user\" --email \"$admin_email\""
    return 0
  fi

  # Generate an initial invite code
  generate_initial_invite_code

  # Clean up
  rm -f "$creds_file"
}

generate_initial_invite_code() {
  log_step "Generating initial invite code"

  echo
  echo "To add additional users, you'll need invite codes."
  echo "An initial invite code will be generated when you start Oqto."
  echo
  echo "After starting the server, create invite codes with:"
  echo "  oqto invites create --uses 1"
  echo
  echo "Or use the web admin interface at:"
  if [[ -n "$DOMAIN" && "$DOMAIN" != "localhost" ]]; then
    echo "  https://${DOMAIN}/admin"
  else
    echo "  http://localhost:8080/admin"
  fi
}

