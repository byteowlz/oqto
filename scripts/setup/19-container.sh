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
    log_info "You can build later with: docker build -f deploy/docker/Dockerfile -t oqto:latest ."
    return
  fi

  cd "$SCRIPT_DIR"

  local dockerfile="deploy/docker/Dockerfile"

  log_info "Building image with $CONTAINER_RUNTIME..."
  $CONTAINER_RUNTIME build -t oqto:latest -f "$dockerfile" .

  log_success "Container image built: oqto:latest"
}
