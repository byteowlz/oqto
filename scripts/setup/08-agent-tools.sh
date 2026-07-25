# ==============================================================================
# ==============================================================================
# Agent Tools Installation
# ==============================================================================
#
# Tools for AI agents in the Oqto platform:
#
#   agntz   - Agent toolkit (wraps other tools, file reservations, etc.)
#   mmry    - Memory storage (workspace-local lexical ledger; no embeddings)
#   trx     - Issue/task tracking
#   scrpr   - Web content extraction (readability, Tavily, Jina)
#   tmpltr  - Document generation from templates (Typst)
#   ignr    - Gitignore generation (auto-detect languages/tools)
#   ast-grep - AST-based structural linting/rewrite engine
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
# Usage: get_dep_version mmry -> "0.4.4"
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
#   binary:  name of the binary to install (e.g., "mmry")
#   repo:    GitHub repo name (e.g., "mmry")
#   package: package name for multi-binary repos (e.g., "mmry-cli")
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

    # Rust repos: repo-vtag-target.tar.gz  (e.g. mmry-v0.4.4-x86_64-unknown-linux-gnu.tar.gz)
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

  # Build cargo install args.
  # Cargo 1.93 removed -p/--package for `cargo install --git`; package must be
  # passed as the optional positional CRATE argument.
  local -a cargo_args=(--git "${BYTEOWLZ_GITHUB}/${repo}.git" --root "$tmpdir")
  if [[ -n "$pkg" ]]; then
    cargo_args+=("$pkg")
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

  # Create a wrapper script that invokes slidev via bun, since the upstream
  # shebang uses #!/usr/bin/env node which may not be installed.
  local slidev_mjs=""
  if [[ -x "$HOME/.bun/bin/slidev" ]]; then
    slidev_mjs=$(readlink -f "$HOME/.bun/bin/slidev" 2>/dev/null)
  fi

  if [[ -n "$slidev_mjs" && -f "$slidev_mjs" ]]; then
    local bun_bin
    bun_bin=$(command -v bun)
    sudo tee "${TOOLS_INSTALL_DIR}/slidev" >/dev/null <<WRAPPER
#!/usr/bin/env bash
exec "$bun_bin" "$slidev_mjs" "\$@"
WRAPPER
    sudo chmod 755 "${TOOLS_INSTALL_DIR}/slidev"
    log_success "Installed slidev wrapper to ${TOOLS_INSTALL_DIR}/slidev"
  elif command_exists slidev; then
    log_success "slidev available at $(command -v slidev)"
  else
    log_warn "slidev installed but could not create global wrapper"
    return 1
  fi

  log_success "slidev installed"
}

install_whisper_cpp() {
  if command_exists whisper-cli || command_exists main && [[ -f "${TOOLS_INSTALL_DIR}/whisper-cli" ]]; then
    log_success "whisper.cpp already installed"
    return 0
  fi

  log_info "Installing whisper.cpp (speech-to-text)..."

  # Check for cmake
  if ! command_exists cmake; then
    log_info "Installing cmake (required by whisper.cpp)..."
    case "$OS_DISTRO" in
    arch | manjaro | endeavouros) sudo pacman -S --noconfirm cmake ;;
    debian | ubuntu | pop | linuxmint) apt_update_once; sudo apt-get install -y cmake ;;
    fedora | centos | rhel | rocky | alma*) sudo dnf install -y cmake ;;
    opensuse*) sudo zypper install -y cmake ;;
    macos) brew install cmake ;;
    *) log_warn "Please install cmake manually"; return 1 ;;
    esac
  fi

  local tmpdir
  tmpdir=$(mktemp -d)
  trap "rm -rf '$tmpdir'" RETURN

  log_info "Cloning whisper.cpp..."
  if ! git clone --depth 1 https://github.com/ggerganov/whisper.cpp.git "$tmpdir/whisper.cpp" 2>/dev/null; then
    log_warn "Failed to clone whisper.cpp"
    return 1
  fi

  cd "$tmpdir/whisper.cpp"

  log_info "Building whisper.cpp..."
  cmake -B build -DCMAKE_BUILD_TYPE=Release 2>&1 | tail -5
  cmake --build build --config Release -j "$(nproc 2>/dev/null || echo 4)" 2>&1 | tail -5

  # The main binary is called 'whisper-cli' in newer versions, 'main' in older
  local binary=""
  if [[ -x build/bin/whisper-cli ]]; then
    binary="build/bin/whisper-cli"
  elif [[ -x build/bin/main ]]; then
    binary="build/bin/main"
  fi

  if [[ -z "$binary" ]]; then
    log_warn "whisper.cpp build failed - no binary found"
    return 1
  fi

  sudo install -m 755 "$binary" "${TOOLS_INSTALL_DIR}/whisper-cli"
  log_success "whisper-cli installed to ${TOOLS_INSTALL_DIR}/whisper-cli"

  # Download a default model (base.en - small and fast, good for English)
  local models_dir="/usr/local/share/whisper/models"
  sudo mkdir -p "$models_dir"

  if [[ ! -f "$models_dir/ggml-base.en.bin" ]]; then
    log_info "Downloading whisper base.en model..."
    local model_url="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
    if sudo curl -fsSL -o "$models_dir/ggml-base.en.bin" "$model_url"; then
      sudo chmod a+r "$models_dir/ggml-base.en.bin"
      log_success "whisper base.en model downloaded"
    else
      log_warn "Failed to download whisper model. Download manually to $models_dir/"
    fi
  else
    log_success "whisper base.en model already present"
  fi

  cd - >/dev/null
}

install_ast_grep() {
  log_step "Installing ast-grep (AST linting)"

  if command_exists ast-grep; then
    log_success "ast-grep already installed"
    return 0
  fi

  local tmpdir
  tmpdir=$(mktemp -d)
  if cargo install --locked --root "$tmpdir" ast-grep 2>&1 | tail -5; then
    if [[ -x "$tmpdir/bin/ast-grep" ]]; then
      install_binary_global "$tmpdir/bin/ast-grep" "ast-grep"
      rm -rf "$tmpdir"
      return 0
    fi
  fi

  rm -rf "$tmpdir"
  log_warn "Failed to install ast-grep automatically (cargo). Install manually: cargo install ast-grep --locked"
  return 1
}

# Locate the repo-root dependency manifest.
find_deps_manifest() {
  local c
  for c in "${SCRIPT_DIR:-}/dependencies.toml" "${SCRIPT_DIR:-}/../dependencies.toml" \
    "${ROOT_DIR:-}/dependencies.toml" "$(pwd)/dependencies.toml"; do
    if [[ -n "$c" && -f "$c" ]]; then
      echo "$c"
      return 0
    fi
  done
  return 1
}

# Install the managed byteowlz agent tools through the single prebuilt,
# checksum-verified acquisition path (`oqto-setup acquire`, ADR-0018/0021). The
# set + pinned versions come from dependencies.toml; this replaces the per-tool
# GitHub-download + cargo/go-build fallbacks for every managed tool. Fail-closed:
# a missing oqto-setup or manifest, or any checksum failure, aborts.
install_managed_agent_tools() {
  log_step "Installing managed agent tools (oqto-setup acquire)"

  if ! command_exists oqto-setup; then
    log_error "oqto-setup not found; run the oqto bootstrap (install.sh) before setup"
    return 1
  fi
  local manifest
  if ! manifest="$(find_deps_manifest)"; then
    log_error "dependencies.toml not found; cannot resolve the managed tool set"
    return 1
  fi
  local arch_arg
  case "$(uname -m)" in
  x86_64 | amd64) arch_arg="x86-64" ;;
  aarch64 | arm64) arch_arg="aarch64" ;;
  *)
    log_warn "unsupported architecture $(uname -m) for prebuilt tools"
    return 1
    ;;
  esac

  local staging
  staging="$(mktemp -d)"
  if sudo oqto-setup acquire --manifest "$manifest" --arch "$arch_arg" \
    --dest "$staging" --install-bin "$TOOLS_INSTALL_DIR"; then
    rm -rf "$staging"
    log_success "Managed agent tools installed to $TOOLS_INSTALL_DIR"
    return 0
  fi
  rm -rf "$staging"
  log_error "oqto-setup acquire failed"
  return 1
}

install_all_agent_tools() {
  log_step "Installing agent tools"

  # External (non-byteowlz) dependencies for agent tools
  install_typst
  install_slidev
  install_whisper_cpp

  # Managed byteowlz tools: one prebuilt, checksum-verified path for the whole
  # set (agntz, mmry(+mcp/service/tui), tmpltr, sldr, ignr, scrpr, sx, trx,
  # skdlr, eavs) resolved from dependencies.toml.
  install_managed_agent_tools

  install_ast_grep || true
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
  echo "    ast-grep - AST-based structural linting"
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

  install_ast_grep || true

  # mmry is the lean append-only memory CLI (mmry-core embedded in the oqto
  # backend; no daemon or embeddings service, per ADR-0010). The managed acquire
  # path installs the CLI set in one shot.
  if [[ "${SELECTED_USER_MODE:-single}" == "multi" || "$INSTALL_MMRY" == "true" ]]; then
    install_managed_agent_tools
  fi

  # Use agntz tools install for additional tools if agntz is available
  if command_exists agntz; then
    log_info "Running agntz doctor to check tool health..."
    agntz tools doctor 2>/dev/null || true
  fi
}

