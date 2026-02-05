# Octo - AI Agent Workspace Platform

default:
    @just --list

# Build all components
build: build-backend build-frontend

# Build backend (all workspace crates)
build-backend:
    cd backend && cargo build

# Build frontend
build-frontend:
    cd frontend && bun run build

# Run all linters
lint: lint-backend lint-frontend

# Lint backend
lint-backend:
    cd backend && cargo clippy && cargo fmt --check

# Lint frontend
lint-frontend:
    cd frontend && bun run lint

# Run all tests
test: test-backend test-frontend

# Test backend
test-backend:
    cd backend && cargo test

# Test frontend
test-frontend:
    cd frontend && bun run test

# Format all Rust code
fmt:
    cd backend && cargo fmt

# Generate TypeScript types from Rust structs
gen-types:
    cd backend && cargo test -p octo export_typescript_bindings -- --nocapture

# Check all Rust code compiles
check:
    cd backend && cargo check

# Start backend server
serve:
    cd backend && cargo run --bin octo -- serve

# Start frontend dev server
dev:
    cd frontend && bun dev

# Start frontend dev server with verbose WS logs and control plane URL
run-frontend:
    cd frontend && VITE_CONTROL_PLANE_URL="http://archlinux:8080" VITE_DEBUG_WS=1 VITE_DEBUG_PI=1 bun dev

# Install all dependencies and binaries
install:
    cd frontend && bun install
    cd backend && cargo install --path crates/octo
    cd backend && cargo install --path crates/octo --bin octo-runner
    cd backend && cargo install --path crates/octo-files
    cd ../hstry && cargo install --path crates/hstry-cli || echo "hstry build failed, skipping"

# Install binaries + systemd unit system-wide (Linux).
#
# - Installs `octo-runner.service` into /usr/lib/systemd/user/
# - Copies local cargo-installed tools into /usr/local/bin
# - Enables lingering for the current user (so user services can run headless)
install-system:
    #!/usr/bin/env bash
    set -euo pipefail

    # Store the octo repo root for later use
    OCTO_ROOT="$(pwd)"

    # Prompt for sudo once up-front
    sudo -v

    just install
    
    # Install sldr binaries (as current user, not sudo - avoids rustup issues with root)
    cd ../sldr && cargo install --path crates/sldr-cli && cargo install --path crates/sldr-server
    
    # Return to octo directory for systemd file installation
    cd "$OCTO_ROOT"

    if [[ "$(uname -s)" != "Linux" ]]; then
      echo "install-system is Linux-only"
      exit 1
    fi

    # Ensure shared group exists and current user is a member
    sudo groupadd -f octo || true
    sudo usermod -a -G octo "$(id -un)" || true

    sudo install -Dm644 deploy/systemd/octo-runner.service /usr/lib/systemd/user/octo-runner.service
    sudo install -Dm644 deploy/systemd/octo-runner.tmpfiles.conf /usr/lib/tmpfiles.d/octo-runner.conf
    sudo systemd-tmpfiles --create /usr/lib/tmpfiles.d/octo-runner.conf || true
    sudo systemctl daemon-reload || true

    # Ensure shared runner socket dir exists for current user
    sudo install -d -m 2770 -o "$(id -un)" -g octo "/run/octo/runner-sockets/$(id -un)" || true

    # System-wide CLI tools.
    #
    # Prefer copying from ~/.cargo/bin (freshly updated by `just install`) so updates
    # are not blocked by PATH precedence.
    for bin in trx mmry mmry-service agntz hstry skdlr octo octo-runner octo-files sldr sldr-server; do
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

    # Enable lingering for current user so `systemctl --user` services can run without login
    sudo loginctl enable-linger "$(id -un)" || true

# Build container image
container-build:
    docker build -t octo-dev:latest -f container/Dockerfile .

# Show backend config
config:
    cd backend && cargo run --bin octo -- config show

# Generate invite codes
invite-codes:
    cd backend && cargo run --bin octo -- invite-codes generate

# Reload backend: build, install, stop, and restart octo serve --local-mode
reload:
    ./scripts/reload-backend.sh

# Reload backend but don't restart server
reload-stop:
    ./scripts/reload-backend.sh --no-start

# Restart system runner socket for current user
restart-runner:
    sudo pkill -f "/usr/local/bin/octo-runner --socket /run/octo/runner-sockets/$(id -un)/octo-runner.sock" || true
    nohup /usr/local/bin/octo-runner --socket "/run/octo/runner-sockets/$(id -un)/octo-runner.sock" >/tmp/octo-runner.log 2>&1 &

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
