# ==============================================================================
# ==============================================================================
# Agent Tools Installation
# ==============================================================================
#
# Tools for AI agents in the Oqto platform:
#
#   agntz   - Agent toolkit (wraps other tools, file reservations, etc.)
#   mmry    - Memory storage and semantic search
#   trx     - Issue/task tracking
#   scrpr   - Web content extraction (readability, Tavily, Jina)
#   tmpltr  - Document generation from templates (Typst)
#   ignr    - Gitignore generation (auto-detect languages/tools)
#
# Installation sources (in order of preference):
#   1. cargo install / go install from registries
#   2. cargo install --git / go install from GitHub
#   3. Local build from source (if available)
#
# ==============================================================================

# GitHub org for byteowlz tools
BYTEOWLZ_GITHUB="https://github.com/byteowlz"

# Global install directory for all agent tools
TOOLS_INSTALL_DIR="/usr/local/bin"

# Read a tool version from dependencies.toml
# Usage: get_dep_version hstry -> "0.4.4"
get_dep_version() {
  local tool="$1"
  local deps_file="${SCRIPT_DIR}/dependencies.toml"
  if [[ ! -f "$deps_file" ]]; then
    echo ""
    return
  fi
  # Simple TOML parser: find "tool = "version"" under [byteowlz] section
  sed -n '/^\[byteowlz\]/,/^\[/{s/^'"$tool"' *= *"\(.*\)"/\1/p}' "$deps_file" | head -1
}

# Detect platform for GitHub release downloads
get_release_target() {
  local arch
  arch=$(uname -m)
  local os
  os=$(uname -s)

  case "$os" in
  Linux)
    case "$arch" in
    x86_64) echo "x86_64-unknown-linux-gnu" ;;
    aarch64) echo "aarch64-unknown-linux-gnu" ;;
    *) echo "" ;;
    esac
    ;;
  Darwin)
    case "$arch" in
    x86_64) echo "x86_64-apple-darwin" ;;
    arm64) echo "aarch64-apple-darwin" ;;
    *) echo "" ;;
    esac
    ;;
  *) echo "" ;;
  esac
}

# Download pre-built binary from GitHub releases.
# Falls back to cargo install if download fails or no release exists.
# Usage: download_or_build_tool <binary> <repo> [package]
#   binary:  name of the binary to install (e.g., "hstry")
#   repo:    GitHub repo name (e.g., "hstry")
#   package: package name for multi-binary repos (e.g., "hstry-cli")
download_or_build_tool() {
  local tool="$1"
  local repo="${2:-$tool}"
  local pkg="${3:-}"      # package name for multi-binary Rust repos
  local lang="${4:-rust}" # "rust" or "go"

  local version
  version=$(get_dep_version "$repo")
  local target
  target=$(get_release_target)

  # Try downloading pre-built binary from GitHub releases
  if [[ -n "$version" && "$version" != "latest" && -n "$target" ]]; then
    local tag="v${version}"
    local tmpdir
    tmpdir=$(mktemp -d)

    # Rust repos: repo-vtag-target.tar.gz  (e.g. hstry-v0.4.4-x86_64-unknown-linux-gnu.tar.gz)
    # Go repos:   repo_OS_arch.tar.gz      (e.g. sx_Linux_x86_64.tar.gz)
    local -a urls=()

    # Rust-style URL
    urls+=("${BYTEOWLZ_GITHUB}/${repo}/releases/download/${tag}/${repo}-${tag}-${target}.tar.gz")

    # Go-style URL (goreleaser convention)
    local go_os go_arch
    case "$target" in
    x86_64-unknown-linux-gnu)
      go_os="Linux"
      go_arch="x86_64"
      ;;
    aarch64-unknown-linux-gnu)
      go_os="Linux"
      go_arch="arm64"
      ;;
    x86_64-apple-darwin)
      go_os="Darwin"
      go_arch="x86_64"
      ;;
    aarch64-apple-darwin)
      go_os="Darwin"
      go_arch="arm64"
      ;;
    esac
    if [[ -n "$go_os" ]]; then
      urls+=("${BYTEOWLZ_GITHUB}/${repo}/releases/download/${tag}/${repo}_${go_os}_${go_arch}.tar.gz")
    fi

    log_info "Downloading $tool $tag for $target..."

    for url in "${urls[@]}"; do
      if curl -fsSL "$url" | tar xz -C "$tmpdir" 2>/dev/null; then
        if [[ -x "$tmpdir/$tool" ]]; then
          install_binary_global "$tmpdir/$tool" "$tool"
          rm -rf "$tmpdir"
          log_success "$tool $tag installed from release"
          return 0
        fi
      fi
    done

    rm -rf "$tmpdir"
    log_info "No pre-built binary available, building from source..."
  fi

  # Fall back to building from source
  if [[ "$lang" == "go" ]]; then
    install_go_tool "$tool" "$repo"
  else
    install_rust_tool "$tool" "$repo" "$pkg"
  fi
}

# Move a built binary into TOOLS_INSTALL_DIR
install_binary_global() {
  local binary_path="$1"
  local tool="$2"

  if [[ ! -f "$binary_path" ]]; then
    log_warn "Binary not found: $binary_path"
    return 1
  fi

  sudo install -m 755 "$binary_path" "${TOOLS_INSTALL_DIR}/${tool}"
  log_success "$tool installed to ${TOOLS_INSTALL_DIR}/${tool}"
}

# Install a Rust tool from crates.io, GitHub, or local source.
# Installs to /usr/local/bin so all users can access it.
install_rust_tool() {
  local tool="$1"
  local repo="${2:-$tool}" # repo name, defaults to tool name
  local pkg="${3:-}"       # package name for multi-binary repos (optional)

  if ! command_exists cargo; then
    log_error "Cargo not available. Cannot install $tool."
    return 1
  fi

  # Always install from git to get the latest compatible version.
  # byteowlz tools are tightly coupled and not published to crates.io.
  log_info "Installing $tool from git (latest)..."

  local tmpdir
  tmpdir=$(mktemp -d)
  trap "rm -rf '$tmpdir'" RETURN

  # Special-case ignr to avoid native-tls by forcing rustls-tls
  if [[ "$tool" == "ignr" && -z "$pkg" ]]; then
    log_info "Installing ignr with rustls-tls (avoids OpenSSL)"
    local ignr_dir="$tmpdir/src"
    if git clone --depth 1 "${BYTEOWLZ_GITHUB}/${repo}.git" "$ignr_dir" 2>/dev/null; then
      perl -pi -e 's/reqwest = \{ version = "0\.12", features = \["blocking"\] \}/reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls"] }/' "$ignr_dir/Cargo.toml"
      if cargo install --path "$ignr_dir" --root "$tmpdir" 2>&1 | tail -5; then
        if [[ -x "$tmpdir/bin/$tool" ]]; then
          install_binary_global "$tmpdir/bin/$tool" "$tool"
          return 0
        fi
      fi
    fi
  fi

  # Build cargo install args
  local -a cargo_args=(--git "${BYTEOWLZ_GITHUB}/${repo}.git" --root "$tmpdir")
  if [[ -n "$pkg" ]]; then
    # Multi-binary repo: specify both the package and the binary name
    cargo_args+=(-p "$pkg" --bin "$tool")
  fi

  # Install from GitHub (always latest main branch)
  if cargo install "${cargo_args[@]}" 2>&1 | tail -5; then
    if [[ -x "$tmpdir/bin/$tool" ]]; then
      install_binary_global "$tmpdir/bin/$tool" "$tool"
      return 0
    fi
  fi

  # Check for local source directory
  local local_path=""
  for base in "$HOME/byteowlz" "$HOME/code/byteowlz" "/opt/byteowlz"; do
    if [[ -d "$base/$repo" ]]; then
      local_path="$base/$repo"
      break
    fi
  done

  if [[ -n "$local_path" && -f "$local_path/Cargo.toml" ]]; then
    log_info "Installing from local path: $local_path"
    local -a local_args=(--root "$tmpdir")
    if [[ -n "$pkg" && -d "$local_path/crates/$pkg" ]]; then
      # Multi-binary workspace: point --path to the specific package crate
      local_args+=(--path "$local_path/crates/$pkg")
    else
      local_args+=(--path "$local_path")
    fi
    if cargo install "${local_args[@]}" 2>&1 | tail -5; then
      if [[ -x "$tmpdir/bin/$tool" ]]; then
        install_binary_global "$tmpdir/bin/$tool" "$tool"
        return 0
      fi
    fi
  fi

  log_warn "Failed to install $tool"
  return 1
}

# Install a Go tool from GitHub or local source.
# Installs to /usr/local/bin so all users can access it.
# Handles both root-level main.go and cmd/<tool>/main.go layouts.
install_go_tool() {
  local tool="$1"
  local repo="${2:-$tool}" # repo name, defaults to tool name
  local go_module="github.com/byteowlz/${repo}"

  if command_exists "$tool"; then
    local version
    version=$("$tool" --version 2>/dev/null | head -1 || echo 'unknown')
    log_success "$tool already installed: $version"
    return 0
  fi

  if ! command_exists go; then
    log_warn "Go not available. Cannot install $tool."
    return 1
  fi

  log_info "Installing $tool..."

  local tmpdir
  tmpdir=$(mktemp -d)
  trap "rm -rf '$tmpdir'" RETURN

  # Try go install from GitHub (root package first, then cmd/<tool>)
  if GOBIN="$tmpdir" go install "${go_module}@latest" 2>/dev/null; then
    install_binary_global "$tmpdir/$tool" "$tool"
    return 0
  fi

  if GOBIN="$tmpdir" go install "${go_module}/cmd/${tool}@latest" 2>/dev/null; then
    install_binary_global "$tmpdir/$tool" "$tool"
    return 0
  fi

  # Check for local source directory
  local local_path=""
  for base in "$HOME/byteowlz" "$HOME/code/byteowlz" "/opt/byteowlz"; do
    if [[ -d "$base/$repo" ]]; then
      local_path="$base/$repo"
      break
    fi
  done

  if [[ -n "$local_path" && -f "$local_path/go.mod" ]]; then
    log_info "Installing from local path: $local_path"
    if [[ -f "$local_path/cmd/$tool/main.go" ]]; then
      if (cd "$local_path" && GOBIN="$tmpdir" go install "./cmd/$tool" 2>/dev/null); then
        install_binary_global "$tmpdir/$tool" "$tool"
        return 0
      fi
    elif (cd "$local_path" && GOBIN="$tmpdir" go install . 2>/dev/null); then
      install_binary_global "$tmpdir/$tool" "$tool"
      return 0
    fi
  fi

  # Fallback: clone repo and build locally (handles mismatched module paths)
  log_info "Trying clone and build..."
  local clone_dir="${tmpdir}/src"
  if git clone --depth 1 "${BYTEOWLZ_GITHUB}/${repo}.git" "$clone_dir" 2>/dev/null; then
    if [[ -f "$clone_dir/cmd/$tool/main.go" ]]; then
      if (cd "$clone_dir" && GOBIN="$tmpdir" go install "./cmd/$tool"); then
        install_binary_global "$tmpdir/$tool" "$tool"
        return 0
      fi
    elif (cd "$clone_dir" && GOBIN="$tmpdir" go install .); then
      install_binary_global "$tmpdir/$tool" "$tool"
      return 0
    fi
  fi

  log_warn "Failed to install $tool"
  return 1
}

install_agntz() {
  log_step "Installing agntz (Agent Toolkit)"
  download_or_build_tool agntz
}

install_typst_from_cargo() {
  if ! command_exists cargo; then
    log_warn "Cargo not available; cannot install typst"
    return 1
  fi

  local tmpdir
  tmpdir=$(mktemp -d)
  trap "rm -rf '$tmpdir'" RETURN

  if cargo install typst-cli --locked --root "$tmpdir" 2>&1 | tail -3; then
    if [[ -x "$tmpdir/bin/typst" ]]; then
      sudo install -m 755 "$tmpdir/bin/typst" "${TOOLS_INSTALL_DIR}/typst"
      log_success "typst installed"
      return 0
    fi
  fi

  log_warn "typst cargo install failed"
  return 1
}

install_typst() {
  # typst is a dependency of tmpltr (document generation)
  if command_exists typst; then
    log_success "typst already installed: $(typst --version 2>/dev/null | head -1)"
    return 0
  fi

  log_info "Installing typst..."
  local arch
  arch=$(uname -m)
  local os
  os=$(uname -s | tr '[:upper:]' '[:lower:]')

  local target=""
  case "${os}-${arch}" in
    linux-x86_64)  target="x86_64-unknown-linux-musl" ;;
    linux-aarch64) target="aarch64-unknown-linux-musl" ;;
    darwin-x86_64) target="x86_64-apple-darwin" ;;
    darwin-arm64)  target="aarch64-apple-darwin" ;;
  esac

  if [[ -z "$target" ]]; then
    log_warn "No pre-built typst for ${os}-${arch}, trying cargo install..."
    install_typst_from_cargo
    return $?
  fi

  local tmpdir
  tmpdir=$(mktemp -d)
  local url="https://github.com/typst/typst/releases/latest/download/typst-${target}.tar.xz"
  if curl -fsSL "$url" -o "$tmpdir/typst.tar.xz" 2>/dev/null; then
    tar -xf "$tmpdir/typst.tar.xz" -C "$tmpdir"
    local bin
    bin=$(find "$tmpdir" -name "typst" -type f | head -1)
    if [[ -n "$bin" ]]; then
      sudo install -m 755 "$bin" "${TOOLS_INSTALL_DIR}/typst"
      log_success "typst installed"
    else
      log_warn "typst binary not found in archive"
    fi
  else
    log_warn "Failed to download typst, trying cargo install..."
    install_typst_from_cargo
  fi
  rm -rf "$tmpdir"
}

install_slidev() {
  # slidev is a dependency of sldr (presentation tool)
  if command_exists slidev; then
    log_success "slidev already installed"
    return 0
  fi

  log_info "Installing slidev (sli.dev)..."
  if command_exists bun; then
    bun install -g @slidev/cli 2>&1 | tail -3
  elif command_exists npm; then
    npm install -g @slidev/cli 2>&1 | tail -3
  else
    log_warn "Neither bun nor npm found, cannot install slidev"
    return 1
  fi

  local slidev_path=""
  if command_exists slidev; then
    slidev_path="$(command -v slidev)"
  elif [[ -x "$HOME/.bun/bin/slidev" ]]; then
    slidev_path="$HOME/.bun/bin/slidev"
  fi

  if [[ -n "$slidev_path" && "$slidev_path" != "${TOOLS_INSTALL_DIR}/slidev" ]]; then
    sudo install -m 755 "$slidev_path" "${TOOLS_INSTALL_DIR}/slidev"
    log_success "Installed slidev to ${TOOLS_INSTALL_DIR}/slidev"
  fi

  log_success "slidev installed"
}

install_all_agent_tools() {
  log_step "Installing agent tools"

  # External dependencies for agent tools
  install_typst
  install_slidev

  # Core tools (Rust) - tries pre-built GitHub release first, falls back to cargo
  # Multi-binary repos need the 3rd arg (package hint) for cargo fallback
  download_or_build_tool hstry hstry hstry-cli
  download_or_build_tool hstry-tui hstry hstry-tui
  download_or_build_tool agntz
  download_or_build_tool mmry mmry mmry-cli
  download_or_build_tool mmry-service mmry mmry-service
  download_or_build_tool tmpltr
  download_or_build_tool sldr sldr sldr-cli
  download_or_build_tool ignr

  # Core tools (Go) - tries pre-built GitHub release first, falls back to go install
  download_or_build_tool scrpr scrpr "" go
  download_or_build_tool sx sx "" go
}

select_agent_tools() {
  log_step "Agent Tools Selection"

  echo
  echo "Oqto can install agent tools:"
  echo
  echo -e "  ${BOLD}Core tools (recommended):${NC}"
  echo "    agntz   - Agent toolkit (file reservations, tool management)"
  echo "    mmry    - Memory storage and semantic search"
  echo "    scrpr   - Web content extraction"
  echo "    sx      - Web search via local SearXNG instance"
  echo
  echo -e "  ${BOLD}Additional tools:${NC}"
  echo "    tmpltr  - Document generation from templates (requires typst)"
  echo "    sldr    - Modular presentations (requires slidev/sli.dev)"
  echo "    ignr    - Gitignore generation"
  echo "    trx     - Issue/task tracking"
  echo
  echo "  Installing sx will also set up a local SearXNG search engine"
  echo "  with Valkey for caching (binds to 127.0.0.1:8888)."
  echo

  if confirm "Install all agent tools (recommended)?"; then
    INSTALL_MMRY="true"
    INSTALL_ALL_TOOLS="true"
  else
    if confirm "Install mmry (memory system)?"; then
      INSTALL_MMRY="true"
    fi
  fi
}

install_agent_tools_selected() {
  log_step "Installing agent tools"

  if [[ "$INSTALL_ALL_TOOLS" == "true" ]]; then
    install_all_agent_tools
    return
  fi

  # hstry is required for per-user chat history (systemd user service)
  download_or_build_tool hstry hstry hstry-cli

  if [[ "$INSTALL_MMRY" == "true" ]]; then
    download_or_build_tool mmry mmry mmry-cli
    download_or_build_tool mmry-service mmry mmry-service
  fi

  # Use agntz tools install for additional tools if agntz is available
  if command_exists agntz; then
    log_info "Running agntz doctor to check tool health..."
    agntz tools doctor 2>/dev/null || true
  fi
}

