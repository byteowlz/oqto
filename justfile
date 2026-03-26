# Oqto - AI Agent Workspace Platform

default:
    @just --list

# Build all components
build: build-backend build-frontend

# Build backend (all workspace crates)
build-backend:
    cd backend && remote-build build --release -p oqto --bin oqto
    cd backend && remote-build build --release -p oqto-runner --bin oqto-runner
    cd backend && remote-build build --release -p oqto-files --bin oqto-files

# Build frontend
build-frontend:
    cd frontend && bun run build

# Run all linters
lint: lint-backend lint-frontend lint-rust-ai-guardrails

# Lint backend
lint-backend:
    cd backend && remote-build clippy && cargo fmt --check

# Lint frontend
lint-frontend:
    cd frontend && bun run lint

# Install ast-grep (advanced structural linting)
install-ast-grep:
    cargo install ast-grep --locked

# Rust AI guardrails via ast-grep (changed files, production scope only)
lint-rust-ai-guardrails:
    ./scripts/lint/rust-ai-guardrails.sh

# Guardrail metrics (all backend crates)
lint-rust-ai-report:
    ./scripts/lint/rust-ai-guardrails-report.py --paths backend/crates

# Guardrail metrics, excluding findings under #[cfg(test)]
lint-rust-ai-report-prod:
    ./scripts/lint/rust-ai-guardrails-report.py --paths backend/crates --exclude-cfg-test

# Guardrail metrics (changed rust files only)
lint-rust-ai-report-changed:
    ./scripts/lint/rust-ai-guardrails-report.py --changed

# Run all tests
test: test-backend test-frontend

# Test backend
test-backend:
    cd backend && remote-build test

# Test frontend
test-frontend:
    cd frontend && bun run test

# Format all Rust code
fmt:
    cd backend && cargo fmt

# Generate TypeScript types from Rust structs
gen-types:
    cd backend && remote-build test -p oqto export_typescript_bindings -- --nocapture

# Check all Rust code compiles
check:
    cd backend && remote-build check

# Start backend server
serve:
    /usr/local/bin/oqto serve

# Start frontend dev server
dev:
    cd frontend && bun dev

# Start frontend dev server with verbose WS logs and control plane URL
run-frontend:
    cd frontend && VITE_CONTROL_PLANE_URL="http://archlinux:8080" VITE_DEBUG_WS=1 VITE_DEBUG_PI=1 bun dev

# Fast dev loop: rebuild backend remotely, install, restart services
reload-fast:
    ./scripts/fast-reload.sh

# Install all dependencies and binaries
install-all:
    cd frontend && bun install
    cd backend/crates/oqto-browserd && bun install && bun run build
    cd backend && cargo install --path crates/oqto
    cd backend && cargo install --path crates/oqto-runner --bin oqto-runner
    cd backend && cargo install --path crates/oqto-files
    cd ../hstry && cargo install --path crates/hstry-cli || echo "hstry build failed, skipping"

# Install a specific crate by name (e.g. just install oqto-browser)
# Available: oqto, oqto-files, oqto-browser, oqto-scaffold, oqto-setup, oqto-usermgr
install crate:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{crate}}" in
        oqto)
            cd backend && cargo install --path crates/oqto
            ;;
        oqto-runner)
            cd backend && cargo install --path crates/oqto-runner --bin oqto-runner
            ;;
        oqtoctl)
            cd backend && cargo install --path crates/oqto --bin oqtoctl
            ;;
        oqto-sandbox)
            cd backend && cargo install --path crates/oqto --bin oqto-sandbox
            ;;
        pi-bridge)
            cd backend && cargo install --path crates/oqto --bin pi-bridge
            ;;
        oqto-files)
            cd backend && cargo install --path crates/oqto-files
            ;;
        oqto-browser)
            (cd backend/crates/oqto-browserd && bun install && bun run build && bunx playwright install --with-deps chromium)
            cd backend && cargo install --path crates/oqto-browser
            ;;
        oqto-scaffold)
            cd backend && cargo install --path crates/oqto-scaffold
            ;;
        oqto-setup)
            cd backend && cargo install --path crates/oqto-setup
            ;;
        oqto-usermgr)
            cd backend && cargo install --path crates/oqto-usermgr
            ;;
        *)
            echo "Unknown crate: {{crate}}"
            echo ""
            echo "Available crates:"
            echo "  oqto           - Main backend server"
            echo "  oqto-runner    - Multi-user process daemon"
            echo "  oqtoctl        - CLI for server management"
            echo "  oqto-sandbox   - Sandbox wrapper"
            echo "  pi-bridge      - HTTP/WebSocket bridge for Pi"
            echo "  oqto-files     - File access server"
            echo "  oqto-browser   - Browser CLI + daemon"
            echo "  oqto-scaffold  - Project scaffolding"
            echo "  oqto-setup     - Setup utility"
            echo "  oqto-usermgr   - User management"
            exit 1
            ;;
    esac

# Install binaries + systemd unit system-wide (Linux).
#
# - Installs `oqto-runner.service` into /usr/lib/systemd/user/
# - Copies local cargo-installed tools into /usr/local/bin
# - Enables lingering for the current user (so user services can run headless)
install-system:
    #!/usr/bin/env bash
    set -euo pipefail

    # Store the oqto repo root for later use
    OQTO_ROOT="$(pwd)"

    # Prompt for sudo once up-front
    sudo -v

    just install-all
    
    # Install sldr binaries (as current user, not sudo - avoids rustup issues with root)
    cd ../sldr && cargo install --path crates/sldr-cli && cargo install --path crates/sldr-server
    
    # Return to oqto directory for systemd file installation
    cd "$OQTO_ROOT"

    if [[ "$(uname -s)" != "Linux" ]]; then
      echo "install-system is Linux-only"
      exit 1
    fi

    # Ensure shared group exists and current user is a member
    sudo groupadd -f oqto || true
    sudo usermod -a -G oqto "$(id -un)" || true

    sudo install -Dm644 deploy/systemd/oqto-runner.service /usr/lib/systemd/user/oqto-runner.service
    sudo install -Dm644 deploy/systemd/hstry.service /usr/lib/systemd/user/hstry.service
    sudo install -Dm644 deploy/systemd/eavs.service /usr/lib/systemd/user/eavs.service
    sudo install -Dm644 deploy/systemd/oqto-runner.tmpfiles.conf /usr/lib/tmpfiles.d/oqto-runner.conf
    sudo systemd-tmpfiles --create /usr/lib/tmpfiles.d/oqto-runner.conf || true
    sudo systemctl daemon-reload || true

    # Ensure shared runner socket dir exists for current user
    sudo install -d -m 2770 -o "$(id -un)" -g oqto "/run/oqto/runner-sockets/$(id -un)" || true

    # System-wide CLI tools.
    #
    # Prefer copying from ~/.cargo/bin (freshly updated by `just install`) so updates
    # are not blocked by PATH precedence.
    for bin in trx mmry mmry-service agntz hstry skdlr oqto oqto-runner oqto-files sldr sldr-server eavs; do
      src="$HOME/.cargo/bin/$bin"
      if [[ ! -x "$src" ]]; then
        src="$(command -v "$bin" || true)"
      fi
      if [[ -z "${src:-}" ]] || [[ ! -x "$src" ]]; then
        echo "warning: $bin not found"
        continue
      fi

      dst="/usr/local/bin/$bin"

      # If destination exists and is identical, skip.
      if [[ -e "$dst" ]] && cmp -s "$src" "$dst"; then
        continue
      fi

      sudo install -m 0755 "$src" "$dst"
    done

    # Install oqto-browserd daemon bundle (dist + node_modules + package.json + bin)
    sudo install -d -m 0755 /usr/local/lib/oqto-browserd
    sudo rsync -a --delete backend/crates/oqto-browserd/dist/ /usr/local/lib/oqto-browserd/dist/
    sudo rsync -a --delete backend/crates/oqto-browserd/node_modules/ /usr/local/lib/oqto-browserd/node_modules/
    sudo install -m 0644 backend/crates/oqto-browserd/package.json /usr/local/lib/oqto-browserd/package.json
    sudo install -d -m 0755 /usr/local/lib/oqto-browserd/bin
    sudo install -m 0755 backend/crates/oqto-browserd/bin/oqto-browserd.js /usr/local/lib/oqto-browserd/bin/oqto-browserd.js
    # Wrapper script that runs from the lib dir so node resolves modules correctly
    printf '#!/usr/bin/env bash\nexec node /usr/local/lib/oqto-browserd/dist/index.js "$@"\n' | sudo tee /usr/local/bin/oqto-browserd > /dev/null
    sudo chmod 0755 /usr/local/bin/oqto-browserd

    # Enable lingering for current user so `systemctl --user` services can run without login
    sudo loginctl enable-linger "$(id -un)" || true

# Build container image
container-build:
    docker build -t oqto-dev:latest -f container/Dockerfile .

# =============================================================================
# Lima (macOS Linux VM wrapper for Docker runtime)
# =============================================================================

lima-up name="oqto":
    ./deploy/lima/bootstrap.sh up {{name}}

lima-ssh name="oqto":
    ./deploy/lima/bootstrap.sh ssh {{name}}

lima-logs name="oqto":
    ./deploy/lima/bootstrap.sh logs {{name}}

lima-status name="oqto":
    ./deploy/lima/bootstrap.sh status {{name}}

lima-down name="oqto":
    ./deploy/lima/bootstrap.sh down {{name}}

# Show backend config
config:
    cd backend && cargo run --bin oqto -- config show

# Generate invite codes
invite-codes:
    cd backend && cargo run --bin oqto -- invite-codes generate

# Fast reload: remote-build + install + restart oqto/runner
reload:
    ./scripts/fast-reload.sh

# Reload backend but don't restart server (legacy)
reload-stop:
    ./scripts/reload-backend.sh --no-start

# Restart system runner socket for current user
restart-runner:
    sudo pkill -f "/usr/local/bin/oqto-runner --socket /run/oqto/runner-sockets/$(id -un)/oqto-runner.sock" || true
    nohup /usr/local/bin/oqto-runner --socket "/run/oqto/runner-sockets/$(id -un)/oqto-runner.sock" >/tmp/oqto-runner.log 2>&1 &

# Build, install, and restart runner + backend
update-runner:
    cd backend && remote-build build --release -p oqto --bin oqto
    cd backend && remote-build build --release -p oqto-runner --bin oqto-runner
    ./scripts/update-runner.sh

# === E2E (Proxmox) ===
e2e-proxmox-lxc-create:
    ./scripts/e2e/proxmox-lxc-create.sh

e2e-proxmox-lxc-login target="ephemeral":
    ./scripts/e2e/proxmox-lxc-login.sh --target {{target}}

e2e-proxmox-prepare:
    ./scripts/e2e/proxmox-prepare.sh

e2e-proxmox-reset target:
    ./scripts/e2e/proxmox-reset.sh --target {{target}}

e2e-proxmox-snapshot target:
    ./scripts/e2e/proxmox-reset.sh --target {{target}} --create-snapshot

e2e-proxmox-setup target mode="toml":
    ./scripts/e2e/proxmox-setup.sh --target {{target}} --mode {{mode}}

e2e-proxmox-test target:
    ./scripts/e2e/proxmox-run-tests.sh --target {{target}}

# Bump version across all components
# Usage: just bump patch|minor|major|x.y.z
bump version:
    #!/usr/bin/env bash
    set -euo pipefail

    ROOT="$(git rev-parse --show-toplevel)"
    
    # Get current version from backend/Cargo.toml
    current=$(grep -m1 '^version = ' "$ROOT/backend/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')
    
    # Parse current version
    IFS='.' read -r major minor patch <<< "$current"
    
    # Calculate new version
    case "{{version}}" in
        patch)
            new_version="$major.$minor.$((patch + 1))"
            ;;
        minor)
            new_version="$major.$((minor + 1)).0"
            ;;
        major)
            new_version="$((major + 1)).0.0"
            ;;
        *)
            # Assume explicit version
            new_version="{{version}}"
            ;;
    esac
    
    echo "Bumping $current -> $new_version"
    
    # Update workspace version in backend/Cargo.toml
    sed -i 's/^version = ".*"/version = "'"$new_version"'"/' "$ROOT/backend/Cargo.toml"
    
    # Update frontend/src-tauri/Cargo.toml
    sed -i '0,/^version = /s/^version = ".*"/version = "'"$new_version"'"/' "$ROOT/frontend/src-tauri/Cargo.toml"
    
    # Update package.json files
    cd "$ROOT/frontend" && bun pm pkg set version="$new_version"
    
    # Update tauri.conf.json
    jq --arg v "$new_version" '.version = $v' "$ROOT/frontend/src-tauri/tauri.conf.json" > "$ROOT/frontend/src-tauri/tauri.conf.json.tmp" \
        && mv "$ROOT/frontend/src-tauri/tauri.conf.json.tmp" "$ROOT/frontend/src-tauri/tauri.conf.json"
    
    echo "Bumped all components to $new_version"

# Git add everything except uploads folder
add:
    git add --all -- ':!uploads/'

# Update external dependencies manifest from local repos and git tags
update-deps:
    #!/usr/bin/env bash
    set -euo pipefail

    ROOT="$(pwd)"
    MANIFEST="$ROOT/dependencies.toml"

    echo "Updating dependencies.toml..."

    # Update Oqto version (under [oqto] section)
    OQTO_VERSION=$(grep -m1 '^version = ' "$ROOT/backend/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')
    sed -i '/^\[oqto\]$/,/^\[/{s/^\(version = \)"[^"]*"/\1"'"$OQTO_VERSION"'"/}' "$MANIFEST"
    echo "  oqto: $OQTO_VERSION"

    # Auto-discover: parse all keys under [byteowlz] section from the manifest,
    # then look for matching sibling repos and read their version.
    # Supports: Cargo.toml, Cargo workspace members, Go modules, pyproject.toml, package.json
    # No hardcoded list to maintain.
    in_section=false
    while IFS= read -r line; do
      # Detect section headers
      if [[ "$line" =~ ^\[byteowlz\]$ ]]; then
        in_section=true
        continue
      elif [[ "$line" =~ ^\[.+\]$ ]]; then
        in_section=false
        continue
      fi

      # Skip if not in [byteowlz] section or line is empty/comment
      $in_section || continue
      [[ "$line" =~ ^[a-zA-Z] ]] || continue

      # Parse key = "value"
      key=$(echo "$line" | sed 's/ *=.*//')
      old_val=$(echo "$line" | sed 's/.*= *"\([^"]*\)".*/\1/')

      # Try to find the repo as a sibling directory (case-insensitive match)
      repo_dir=""
      for candidate in "$ROOT/../$key" "$ROOT/../$(echo "$key" | tr '[:upper:]' '[:lower:]')"; do
        if [[ -d "$candidate" ]]; then
          repo_dir="$candidate"
          break
        fi
      done

      if [[ -z "$repo_dir" ]]; then
        echo "  $key: $old_val (no sibling repo found)"
        continue
      fi

      # Extract version from project files
      version=""

      if [[ -f "$repo_dir/Cargo.toml" ]]; then
        # Try direct version first
        version=$(grep -m1 '^version = ' "$repo_dir/Cargo.toml" | sed 's/version = "\(.*\)"/\1/' || true)

        # If empty, it's a workspace -- look in the member matching the repo name
        if [[ -z "$version" ]]; then
          for member_dir in "$repo_dir/$key" "$repo_dir/$(echo "$key" | tr '[:upper:]' '[:lower:]')"; do
            if [[ -f "$member_dir/Cargo.toml" ]]; then
              version=$(grep -m1 '^version = ' "$member_dir/Cargo.toml" | sed 's/version = "\(.*\)"/\1/' || true)
              [[ -n "$version" ]] && break
            fi
          done
        fi

        # Still empty? Try workspace.package.version
        if [[ -z "$version" ]]; then
          version=$(sed -n '/\[workspace.package\]/,/^\[/p' "$repo_dir/Cargo.toml" \
            | grep -m1 '^version = ' | sed 's/version = "\(.*\)"/\1/' || true)
        fi
      fi

      # Go modules: grep for version constant, then fall back to git tags
      if [[ -z "$version" && -f "$repo_dir/go.mod" ]]; then
        # Search common locations for version = "x.y.z" or Version = "x.y.z"
        version=$(grep -rh '[vV]ersion\s*=\s*"[0-9]' "$repo_dir"/*.go "$repo_dir"/cmd/ "$repo_dir"/internal/ 2>/dev/null \
          | grep -m1 -oP '"\K[0-9]+\.[0-9]+\.[0-9]+' || true)
        # Fallback: latest git tag
        if [[ -z "$version" ]]; then
          version=$(cd "$repo_dir" && git describe --tags --abbrev=0 2>/dev/null | sed 's/^v//' || true)
        fi
      fi

      # Python / JS fallbacks
      if [[ -z "$version" && -f "$repo_dir/pyproject.toml" ]]; then
        version=$(grep -m1 '^version = ' "$repo_dir/pyproject.toml" | sed 's/version = "\(.*\)"/\1/' || true)
      fi
      if [[ -z "$version" && -f "$repo_dir/package.json" ]]; then
        version=$(jq -r '.version // empty' "$repo_dir/package.json" 2>/dev/null || true)
      fi

      if [[ -n "$version" ]]; then
        sed -i "s/^\($key = \)\"[^\"]*\"/\1\"$version\"/" "$MANIFEST"
        if [[ "$version" != "$old_val" ]]; then
          echo "  $key: $old_val -> $version"
        else
          echo "  $key: $version (unchanged)"
        fi
      else
        echo "  $key: $old_val (could not detect version in $repo_dir)"
      fi
    done < "$MANIFEST"

    echo ""
    echo "Done: dependencies.toml updated"

# Install/update all byteowlz dependencies from sibling source repos
install-deps *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail

    ROOT="$(pwd)"
    MANIFEST="$ROOT/dependencies.toml"
    FILTER="{{ARGS}}"  # Optional: install only a specific tool

    # Parse [byteowlz] entries from manifest
    declare -A TOOLS
    in_section=false
    while IFS= read -r line; do
      if [[ "$line" =~ ^\[byteowlz\]$ ]]; then in_section=true; continue; fi
      if [[ "$line" =~ ^\[.+\]$ ]]; then in_section=false; continue; fi
      $in_section || continue
      [[ "$line" =~ ^[a-zA-Z] ]] || continue
      key=$(echo "$line" | sed 's/ *=.*//')
      ver=$(echo "$line" | sed 's/.*= *"\([^"]*\)".*/\1/')
      TOOLS[$key]="$ver"
    done < "$MANIFEST"

    PIDS=()
    NAMES=()
    LOGS=()

    for key in "${!TOOLS[@]}"; do
      manifest_ver="${TOOLS[$key]}"

      # If filter is set, only install that tool
      if [[ -n "$FILTER" && "$key" != "$FILTER" ]]; then
        continue
      fi

      # Find repo directory (case-insensitive)
      repo_dir=""
      for candidate in "$ROOT/../$key" "$ROOT/../$(echo "$key" | tr '[:upper:]' '[:lower:]')"; do
        [[ -d "$candidate" ]] && repo_dir="$candidate" && break
      done

      if [[ -z "$repo_dir" || ! -f "$repo_dir/Cargo.toml" ]]; then
        echo "SKIP $key: no sibling repo found"
        continue
      fi

      # Map tool name -> binary name where they differ
      bin="$key"
      case "$key" in
        eaRS) bin="ears" ;;
        kokorox) bin="koko" ;;
        mailz) bin="mailz-cli" ;;
      esac
      installed_ver=$($bin --version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
      [[ -z "$installed_ver" ]] && installed_ver=$($bin version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)

      if [[ "$installed_ver" == "$manifest_ver" && -z "$FILTER" ]]; then
        echo "OK   $key ($manifest_ver already installed)"
        continue
      fi

      # Determine install path
      install_path=""
      if grep -q '^\[workspace\]' "$repo_dir/Cargo.toml"; then
        # Workspace: find the CLI crate
        lkey=$(echo "$key" | tr '[:upper:]' '[:lower:]')
        for crate_dir in "$repo_dir/crates/${lkey}-cli" "$repo_dir/crates/$lkey" "$repo_dir/${lkey}-cli" "$repo_dir/$lkey"; do
          if [[ -f "$crate_dir/Cargo.toml" && -f "$crate_dir/src/main.rs" ]]; then
            install_path="$crate_dir"
            break
          fi
        done
        # Fallback: find first workspace member with a main.rs
        if [[ -z "$install_path" ]]; then
          for member_dir in "$repo_dir"/*/; do
            if [[ -f "$member_dir/src/main.rs" && -f "$member_dir/Cargo.toml" ]]; then
              install_path="$member_dir"
              break
            fi
          done
        fi
        if [[ -z "$install_path" ]]; then
          echo "SKIP $key: workspace but no CLI crate found"
          continue
        fi
      else
        install_path="$repo_dir"
      fi

      logfile=$(mktemp)
      echo "BUILD $key ($installed_ver -> $manifest_ver) ..."
      cargo install --path "$install_path" --force > "$logfile" 2>&1 &
      PIDS+=($!)
      NAMES+=("$key")
      LOGS+=("$logfile")
    done

    # Wait for all builds
    FAILED=0
    for i in "${!PIDS[@]}"; do
      if wait "${PIDS[$i]}"; then
        echo "DONE ${NAMES[$i]}"
      else
        echo "FAIL ${NAMES[$i]} (see log below)"
        cat "${LOGS[$i]}"
        FAILED=$((FAILED + 1))
      fi
      rm -f "${LOGS[$i]}"
    done

    if [[ $FAILED -gt 0 ]]; then
      echo ""
      echo "$FAILED tool(s) failed to install"
      exit 1
    fi

    echo ""
    echo "All dependencies installed"

# Check installed binary versions against dependencies.toml
check-deps:
    #!/usr/bin/env bash
    set -euo pipefail

    MANIFEST="$(pwd)/dependencies.toml"
    OK=0
    WARN=0
    MISS=0

    printf "%-12s %-12s %-12s %s\n" "TOOL" "MANIFEST" "INSTALLED" "STATUS"
    printf "%-12s %-12s %-12s %s\n" "----" "--------" "---------" "------"

    # Check oqto
    OQTO_MANIFEST=$(sed -n '/^\[oqto\]/,/^\[/{s/^version = "\(.*\)"/\1/p}' "$MANIFEST")
    OQTO_INSTALLED=$(grep -m1 '^version = ' backend/Cargo.toml | sed 's/version = "\(.*\)"/\1/')
    if [[ "$OQTO_MANIFEST" == "$OQTO_INSTALLED" ]]; then
      printf "%-12s %-12s %-12s %s\n" "oqto" "$OQTO_MANIFEST" "$OQTO_INSTALLED" "ok"
      OK=$((OK + 1))
    else
      printf "%-12s %-12s %-12s %s\n" "oqto" "$OQTO_MANIFEST" "$OQTO_INSTALLED" "MISMATCH"
      WARN=$((WARN + 1))
    fi

    # Check all [byteowlz] entries
    in_section=false
    while IFS= read -r line; do
      if [[ "$line" =~ ^\[byteowlz\]$ ]]; then in_section=true; continue; fi
      if [[ "$line" =~ ^\[.+\]$ ]]; then in_section=false; continue; fi
      $in_section || continue
      [[ "$line" =~ ^[a-zA-Z] ]] || continue

      key=$(echo "$line" | sed 's/ *=.*//')
      manifest_ver=$(echo "$line" | sed 's/.*= *"\([^"]*\)".*/\1/')

      # Get installed version (try --version, then version subcommand)
      # Map tool name -> binary name where they differ
      bin="$key"
      case "$key" in
        eaRS) bin="ears" ;;
        kokorox) bin="koko" ;;
        mailz) bin="mailz-cli" ;;
      esac
      installed_ver=$($bin --version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
      if [[ -z "$installed_ver" ]]; then
        installed_ver=$($bin version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
      fi

      if [[ -z "$installed_ver" ]]; then
        printf "%-12s %-12s %-12s %s\n" "$key" "$manifest_ver" "-" "NOT INSTALLED"
        MISS=$((MISS + 1))
      elif [[ "$manifest_ver" == "$installed_ver" ]]; then
        printf "%-12s %-12s %-12s %s\n" "$key" "$manifest_ver" "$installed_ver" "ok"
        OK=$((OK + 1))
      else
        printf "%-12s %-12s %-12s %s\n" "$key" "$manifest_ver" "$installed_ver" "MISMATCH"
        WARN=$((WARN + 1))
      fi
    done < "$MANIFEST"

    echo ""
    echo "Summary: $OK ok, $WARN mismatches, $MISS not installed"

    if [[ $WARN -gt 0 || $MISS -gt 0 ]]; then
      echo ""
      echo "To update installed binaries from source repos:"
      echo "  cd ../<tool> && cargo install --path ."
      exit 1
    fi

# Install git hooks (uses .githooks)
install-hooks:
    #!/usr/bin/env bash
    set -euo pipefail

    ROOT="$(pwd)"

    if [[ ! -d "$ROOT/.githooks" ]]; then
      echo "No .githooks directory found"
      exit 1
    fi

    chmod +x "$ROOT/.githooks/pre-commit"
    git config core.hooksPath .githooks
    echo "Git hooks installed"

# Sync schema artifacts to oqto-website
sync-website-schemas:
    #!/usr/bin/env bash
    set -euo pipefail

    ./scripts/sync-oqto-website.sh

# Check whether oqto-website schema artifacts are in sync
check-website-schemas:
    #!/usr/bin/env bash
    set -euo pipefail

    ./scripts/check-oqto-website.sh

# Check for updates to external dependencies
check-updates:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "Checking for external dependency updates..."
    echo ""

    if ! command -v git &> /dev/null; then
        echo "Error: git is required"
        exit 1
    fi

    # Helper function to get version safely
    get_version() {
        local file="$1"
        grep -m1 '^version = ' "$file" 2>/dev/null | sed 's/version = "\(.*\)"/\1/' || echo ""
    }

    # Array of repos to check
    # Note: pi and sx are excluded - pi is from crates.io, sx has no tags
    declare -A REPOS=(
        ["byteowlz/hstry"]="hstry"
        ["byteowlz/mmry"]="mmry"
        ["byteowlz/trx"]="trx"
        ["byteowlz/agntz"]="agntz"
        ["byteowlz/mailz"]="mailz"
        ["byteowlz/sldr"]="sldr"
        ["byteowlz/eaRS"]="eaRS"
        ["byteowlz/kokorox"]="kokorox"
    )

    for repo_path in "${!REPOS[@]}"; do
        REPO_NAME="${REPOS[$repo_path]}"
        REPO_DIR="../$REPO_NAME"

        echo "=== $REPO_NAME ==="

        # Get local version if repo exists locally
        if [[ -f "$REPO_DIR/Cargo.toml" ]]; then
            LOCAL_VERSION=$(get_version "$REPO_DIR/Cargo.toml")

            # If workspace Cargo.toml doesn't have a version, check member crates
            if [[ -z "$LOCAL_VERSION" ]]; then
                # For workspaces, try to find version in the first member crate
                FIRST_MEMBER=$(grep '^members' "$REPO_DIR/Cargo.toml" | head -1 | sed 's/.*\[\"\([^\"]*\).*/\1/')
                if [[ -n "$FIRST_MEMBER" && -f "$REPO_DIR/$FIRST_MEMBER/Cargo.toml" ]]; then
                    LOCAL_VERSION=$(get_version "$REPO_DIR/$FIRST_MEMBER/Cargo.toml")
                fi
            fi

            # If still no version, mark as unknown
            if [[ -z "$LOCAL_VERSION" ]]; then
                LOCAL_VERSION="unknown"
            fi

            echo "  Local: $LOCAL_VERSION"

            # Get latest tag from GitHub
            if REMOTE_TAG=$(git ls-remote --tags https://github.com/$repo_path 2>/dev/null | tail -1 | sed 's/.*refs\/tags\///' | sed 's/\^{}//'); then
                if [[ -n "$REMOTE_TAG" ]]; then
                    echo "  Remote: $REMOTE_TAG"

                    # Normalize versions (remove 'v' prefix for comparison)
                    if [[ "$LOCAL_VERSION" != "unknown" ]]; then
                        LOCAL_NORMALIZED="${LOCAL_VERSION#v}"
                        REMOTE_NORMALIZED="${REMOTE_TAG#v}"

                        if [[ "$LOCAL_NORMALIZED" != "$REMOTE_NORMALIZED" ]]; then
                            # Use simple string comparison - not perfect for semver but works for basic checks
                            if [[ "$LOCAL_NORMALIZED" < "$REMOTE_NORMALIZED" ]]; then
                                echo "  Status: UPDATE AVAILABLE"
                            else
                                echo "  Status: AHEAD of remote"
                            fi
                        else
                            echo "  Status: Up to date"
                        fi
                    else
                        echo "  Status: (cannot compare - local version unknown)"
                    fi
                else
                    echo "  Remote: (no tags found)"
                fi
            else
                echo "  Remote: (failed to fetch)"
            fi
        else
            echo "  Local: (not found locally)"

            # Still show remote version
            if REMOTE_TAG=$(git ls-remote --tags https://github.com/$repo_path 2>/dev/null | tail -1 | sed 's/.*refs\/tags\///' | sed 's/\^{}//'); then
                if [[ -n "$REMOTE_TAG" ]]; then
                    echo "  Remote: $REMOTE_TAG"
                fi
            fi
        fi

        echo ""
    done

    echo "Note: pi is installed from npm as @mariozechner/pi-coding-agent, sx has no tags yet"

# Release: bump version, commit, and tag
# Usage: just release patch|minor|major|x.y.z
release version:
    #!/usr/bin/env bash
    set -euo pipefail
    
    # Check for uncommitted changes
    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "Error: uncommitted changes exist. Commit or stash them first."
        exit 1
    fi
    
    # Get current version to calculate new one for commit message
    current=$(grep -m1 '^version = ' backend/Cargo.toml | sed 's/version = "\(.*\)"/\1/')
    IFS='.' read -r major minor patch <<< "$current"
    
    case "{{version}}" in
        patch) new_version="$major.$minor.$((patch + 1))" ;;
        minor) new_version="$major.$((minor + 1)).0" ;;
        major) new_version="$((major + 1)).0.0" ;;
        *) new_version="{{version}}" ;;
    esac
    
    # Bump versions
    just bump {{version}}
    
    # Commit and tag
    git add -A
    git commit -m "release: v$new_version"
    git tag -a "v$new_version" -m "Release v$new_version"
    
    echo ""
    echo "Released v$new_version"
    echo "Run 'git push && git push --tags' to publish"

# =============================================================================
# Deployment
# =============================================================================

# Deploy to all configured hosts (build + upload + restart)
deploy *ARGS:
    ./scripts/deploy.sh {{ARGS}}

# Deploy to a specific host only
deploy-host name *ARGS:
    ./scripts/deploy.sh --host {{name}} {{ARGS}}

# Deploy without rebuilding (use existing artifacts)
deploy-quick *ARGS:
    ./scripts/deploy.sh --skip-build {{ARGS}}

# Deploy only backend binaries (skip frontend)
deploy-backend *ARGS:
    ./scripts/deploy.sh --skip-frontend {{ARGS}}

# Deploy only frontend (skip backend binaries)
deploy-frontend *ARGS:
    ./scripts/deploy.sh --skip-backend {{ARGS}}

# Show what deploy would do without doing it
deploy-dry-run *ARGS:
    ./scripts/deploy.sh --dry-run {{ARGS}}

# =============================================================================
# VM Deployment Testing (Proxmox)
# =============================================================================

# Run VM deployment tests on Proxmox
vm-test *ARGS:
    cd scripts && ./test-vm-deployment.sh {{ARGS}}

# List available VM test scenarios
vm-test-list:
    cd scripts && ./test-vm-deployment.sh --list

# Prepare cloud images for VM testing
vm-test-prepare:
    cd scripts && ./test-vm-deployment.sh --prepare-images

# Run a specific VM test scenario (e.g., just vm-test-scenario ubuntu-24-04-local-single)
vm-test-scenario name:
    cd scripts && ./test-vm-deployment.sh --scenario {{name}}

# Clean up all VM test instances
vm-test-cleanup:
    cd scripts && ./test-vm-deployment.sh --cleanup-all

# Reliability API suite (headless, CI-safe)
# Example:
# just reliability --base-url https://oqto.engineeringautomation.eu --username admin --password secret --shared-workspace-id sw_x --workspace-path /home/... --media-path file.mp4
reliability *ARGS:
    ./scripts/e2e/reliability-suite.sh {{ARGS}}

# Reliability soak run (default 30 minutes)
reliability-soak *ARGS:
    ./scripts/e2e/reliability-suite.sh --duration-sec 1800 {{ARGS}}

# Reliability run using local gitignored secrets profile
reliability-local *ARGS:
    ./scripts/e2e/reliability-run-local.sh {{ARGS}}

# Browser journey reliability run (tabs/routes + media checks)
reliability-browser-local *ARGS:
    ./scripts/e2e/reliability-browser-journey.sh {{ARGS}}

# Shared workspace lifecycle reliability checks (create/update/delete loops)
reliability-shared-local *ARGS:
    ./scripts/e2e/reliability-shared-workspaces.sh {{ARGS}}

# Files channel reliability checks (WS mux file mutation loops)
reliability-files-local *ARGS:
    ./scripts/e2e/reliability-files-local.sh {{ARGS}}

# Run the current local reliability baseline suite end-to-end
reliability-all-local:
    ./scripts/e2e/reliability-run-local.sh --duration-sec 300 --interval-sec 5
    ./scripts/e2e/reliability-shared-workspaces.sh --loops 3
    ./scripts/e2e/reliability-files-local.sh --loops 5
    ./scripts/e2e/reliability-browser-journey.sh --wait-ms 1500

# Runner-path smoke checks for backend refactor baseline hardening
smoke-runner-user-plane:
    ./scripts/e2e/smoke-runner-user-plane.sh

# Convert oqto.setup.toml to vm.tests.toml format
vm-convert-config setup_file:
    cd scripts && ./convert-setup-toml.sh {{setup_file}}

# =============================================================================
# Admin Tasks
# =============================================================================

# Run oqto-admin (wrapper for all admin tasks)
admin *ARGS:
    ./scripts/admin/oqto-admin {{ARGS}}

# Show user provisioning status
admin-status *ARGS:
    ./scripts/admin/oqto-admin user-status {{ARGS}}

# Provision EAVS keys for users (--all or --user <name>)
admin-eavs *ARGS:
    ./scripts/admin/oqto-admin eavs-provision {{ARGS}}

# Sync Pi configuration to users (--all or --user <name>)
admin-sync-pi *ARGS:
    ./scripts/admin/oqto-admin sync-pi-config {{ARGS}}

# Manage skills for users (--list, --install <name>, --update)
admin-skills *ARGS:
    ./scripts/admin/oqto-admin manage-skills {{ARGS}}

# Manage bootstrap document templates (--sync, --list, --deploy)
admin-templates *ARGS:
    ./scripts/admin/oqto-admin manage-templates {{ARGS}}

# Full sync: eavs + pi config + skills for all users
admin-sync-all *ARGS:
    ./scripts/admin/oqto-admin sync-all {{ARGS}}

# Update Pi coding agent to latest version (system-wide)
update-pi:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Updating Pi coding agent..."
    bun install -g @mariozechner/pi-coding-agent@latest
    # Re-run system-wide install from setup
    source scripts/setup/05-install-core.sh
    ensure_bun_and_pi_global
    echo "Restarting oqto-runner..."
    systemctl --user restart oqto-runner
    echo "Done. Pi version: $(/usr/local/bin/pi --version 2>/dev/null)"

# --- iOS (remote Mac build via Tailscale) ---

ios-mac := "mac"
ios-remote-dir := "~/byteowlz/oqto"
ios-device := "D9CV2YHT7J"

# Sync project to Mac
ios-sync:
    rsync -avz --delete \
      --exclude 'target/' \
      --exclude 'node_modules/' \
      --exclude 'frontend/src-tauri/gen/' \
      --exclude '.git/' \
      --exclude 'backend/target/' \
      --exclude 'fileserver/target/' \
      ./ {{ios-mac}}:{{ios-remote-dir}}/

# Build iOS app on Mac
ios-build: ios-sync
    ssh -t {{ios-mac}} "cd {{ios-remote-dir}}/frontend && bun install && bun tauri ios build --config src-tauri/tauri.ios.conf.json"

# Build and install to iPhone
ios-deploy: ios-build
    ssh -t {{ios-mac}} "xcrun devicectl device install app --device {{ios-device}} {{ios-remote-dir}}/frontend/src-tauri/gen/apple/build/arm64/oqto.ipa"

# Install last build to iPhone (skip rebuild)
ios-install:
    ssh -t {{ios-mac}} "xcrun devicectl device install app --device {{ios-device}} {{ios-remote-dir}}/frontend/src-tauri/gen/apple/build/arm64/oqto.ipa"


