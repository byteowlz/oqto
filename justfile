# Octo - AI Agent Workspace Platform

default:
    @just --list

# Build all components
build: build-backend build-frontend

# Build backend (all workspace crates)
build-backend:
    cd backend && remote-build build --release -p octo --bin octo --bin octo-runner
    cd backend && remote-build build --release -p octo-files --bin octo-files

# Build frontend
build-frontend:
    cd frontend && bun run build

# Run all linters
lint: lint-backend lint-frontend

# Lint backend
lint-backend:
    cd backend && remote-build clippy && cargo fmt --check

# Lint frontend
lint-frontend:
    cd frontend && bun run lint

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
    cd backend && remote-build test -p octo export_typescript_bindings -- --nocapture

# Check all Rust code compiles
check:
    cd backend && remote-build check

# Start backend server
serve:
    /usr/local/bin/octo serve

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
install:
    cd frontend && bun install
    cd backend/crates/octo-browserd && bun install && bun run build
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

    # Install octo-browserd daemon bundle (dist + node_modules + package.json + bin)
    sudo install -d -m 0755 /usr/local/lib/octo-browserd
    sudo rsync -a --delete backend/crates/octo-browserd/dist/ /usr/local/lib/octo-browserd/dist/
    sudo rsync -a --delete backend/crates/octo-browserd/node_modules/ /usr/local/lib/octo-browserd/node_modules/
    sudo install -m 0644 backend/crates/octo-browserd/package.json /usr/local/lib/octo-browserd/package.json
    sudo install -d -m 0755 /usr/local/lib/octo-browserd/bin
    sudo install -m 0755 backend/crates/octo-browserd/bin/octo-browserd.js /usr/local/lib/octo-browserd/bin/octo-browserd.js
    # Wrapper script that runs from the lib dir so node resolves modules correctly
    printf '#!/usr/bin/env bash\nexec node /usr/local/lib/octo-browserd/dist/index.js "$@"\n' | sudo tee /usr/local/bin/octo-browserd > /dev/null
    sudo chmod 0755 /usr/local/bin/octo-browserd

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

# Fast reload: remote-build + install + restart octo/runner
reload:
    ./scripts/fast-reload.sh

# Reload backend but don't restart server (legacy)
reload-stop:
    ./scripts/reload-backend.sh --no-start

# Restart system runner socket for current user
restart-runner:
    sudo pkill -f "/usr/local/bin/octo-runner --socket /run/octo/runner-sockets/$(id -un)/octo-runner.sock" || true
    nohup /usr/local/bin/octo-runner --socket "/run/octo/runner-sockets/$(id -un)/octo-runner.sock" >/tmp/octo-runner.log 2>&1 &

# Build, install, and restart runner + backend
update-runner:
    cd backend && remote-build build --release -p octo --bin octo --bin octo-runner
    ./scripts/update-runner.sh

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

    # Get current Octo version
    OCTO_VERSION=$(grep -m1 '^version = ' "$ROOT/backend/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')

    # Array of byteowlz repos with their TOML section paths
    declare -A REPOS=(
        ["hstry"]="byteowlz.hstry"
        ["mmry"]="byteowlz.mmry"
        ["trx"]="byteowlz.trx"
        ["agntz"]="byteowlz.agntz"
        ["mailz"]="byteowlz.mailz"
        ["sldr"]="byteowlz.sldr"
        ["eaRS"]="byteowlz.eaRS"
    )

    # Update versions from local repos
    for repo in "${!REPOS[@]}"; do
        REPO_PATH="$ROOT/../$repo"
        TOML_PATH="${REPOS[$repo]}"

        if [[ -f "$REPO_PATH/Cargo.toml" ]]; then
            VERSION=$(grep -m1 '^version = ' "$REPO_PATH/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')
            # Update in place using sed
            sed -i "s/^\($TOML_PATH =\) \"[^\"]*\"/\1 \"$VERSION\"/" "$MANIFEST"
            echo "  $repo: $VERSION"
        else
            echo "  $repo: (not found locally, keeping existing value)"
        fi
    done

    # Update Octo version
    sed -i 's/^\(octo.version =\) "[^"]*"/\1 "'"$OCTO_VERSION"'"/' "$MANIFEST"
    echo "  octo: $OCTO_VERSION"

    # Optional: Fetch latest tags for external repos (requires network)
    if command -v git &> /dev/null; then
        echo ""
        echo "Fetching latest tags from GitHub..."

        # kokorox (byteowlz/kokorox)
        if git ls-remote --tags https://github.com/byteowlz/kokorox &> /dev/null; then
            KOKOROX_LATEST=$(git ls-remote --tags https://github.com/byteowlz/kokorox | tail -1 | sed 's/.*refs\/tags\///' | sed 's/\^{}//')
            if [[ -n "$KOKOROX_LATEST" ]]; then
                sed -i 's/^\(kokorox =\) "[^"]*"/\1 "'"$KOKOROX_LATEST"'"/' "$MANIFEST"
                echo "  kokorox: $KOKOROX_LATEST (from GitHub)"
            fi
        fi

        # Note: pi and sx are not fetched from GitHub
        # - pi: Installed from crates.io, repo doesn't exist on GitHub
        # - sx: Repo exists but has no tags yet
    fi

    echo ""
    echo "Done: dependencies.toml updated"

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
