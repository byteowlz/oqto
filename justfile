# Oqto - AI Agent Workspace Platform

default:
    @just --list

# Build all components
build: build-backend build-frontend

# Build backend (all workspace crates)
build-backend:
    cd backend && remote-build build --release -p oqto --bin oqto --bin oqto-runner
    cd backend && remote-build build --release -p oqto-files --bin oqto-files

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
install:
    cd frontend && bun install
    cd backend/crates/oqto-browserd && bun install && bun run build
    cd backend && cargo install --path crates/oqto
    cd backend && cargo install --path crates/oqto --bin oqto-runner
    cd backend && cargo install --path crates/oqto-files
    cd ../hstry && cargo install --path crates/hstry-cli || echo "hstry build failed, skipping"

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

    just install
    
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
    sudo install -Dm644 deploy/systemd/oqto-runner.tmpfiles.conf /usr/lib/tmpfiles.d/oqto-runner.conf
    sudo systemd-tmpfiles --create /usr/lib/tmpfiles.d/oqto-runner.conf || true
    sudo systemctl daemon-reload || true

    # Ensure shared runner socket dir exists for current user
    sudo install -d -m 2770 -o "$(id -un)" -g oqto "/run/oqto/runner-sockets/$(id -un)" || true

    # System-wide CLI tools.
    #
    # Prefer copying from ~/.cargo/bin (freshly updated by `just install`) so updates
    # are not blocked by PATH precedence.
    for bin in trx mmry mmry-service agntz hstry skdlr oqto oqto-runner oqto-files sldr sldr-server; do
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
    cd backend && remote-build build --release -p oqto --bin oqto --bin oqto-runner
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

    # Update Oqto version
    OQTO_VERSION=$(grep -m1 '^version = ' "$ROOT/backend/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')
    sed -i 's/^\(oqto.version =\) "[^"]*"/\1 "'"$OQTO_VERSION"'"/' "$MANIFEST"
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
