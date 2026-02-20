# ==============================================================================
# Container Image Build
# ==============================================================================

build_container_image() {
  if [[ "$SELECTED_BACKEND_MODE" != "container" ]]; then
    return
  fi

  log_step "Building container image"

  if ! confirm "Build the Oqto container image? (this may take several minutes)"; then
    log_info "Skipping container build"
    log_info "You can build later with: just container-build"
    return
  fi

  cd "$SCRIPT_DIR"

  local dockerfile="container/Dockerfile"
  if [[ "$ARCH" == "arm64" || "$ARCH" == "aarch64" ]]; then
    if [[ -f "container/Dockerfile.arm64" ]]; then
      dockerfile="container/Dockerfile.arm64"
    fi
  fi

  log_info "Building image with $CONTAINER_RUNTIME..."
  $CONTAINER_RUNTIME build -t oqto-dev:latest -f "$dockerfile" .

  log_success "Container image built: oqto-dev:latest"
}

