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

# Check all Rust code compiles
check:
    cd backend && cargo check

# Start backend server
serve:
    cd backend && cargo run --bin octo -- serve

# Start frontend dev server
dev:
    cd frontend && bun dev

# Install all dependencies and binaries
install:
    cd frontend && bun install
    cd backend && cargo install --path crates/octo
    cd backend && cargo install --path crates/octo --bin octo-runner
    cd backend && cargo install --path crates/octo-files

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
    cd "$ROOT/pi-extension" && bun pm pkg set version="$new_version"
    
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
