# Octo - AI Agent Workspace Platform

default:
    @just --list

# Build all components
build: build-backend build-fileserver build-frontend

# Build backend
build-backend:
    cd backend && cargo build

# Build fileserver
build-fileserver:
    cd fileserver && cargo build

# Build frontend
build-frontend:
    cd frontend && bun run build

# Run all linters
lint: lint-backend lint-fileserver lint-frontend

# Lint backend
lint-backend:
    cd backend && cargo clippy && cargo fmt --check

# Lint fileserver
lint-fileserver:
    cd fileserver && cargo clippy && cargo fmt --check

# Lint frontend
lint-frontend:
    cd frontend && bun run lint

# Run all tests
test: test-backend test-fileserver test-frontend

# Test backend
test-backend:
    cd backend && cargo test

# Test fileserver
test-fileserver:
    cd fileserver && cargo test

# Test frontend
test-frontend:
    cd frontend && bun run test

# Format all Rust code
fmt:
    cd backend && cargo fmt
    cd fileserver && cargo fmt

# Check all Rust code compiles
check:
    cd backend && cargo check
    cd fileserver && cargo check

# Start backend server
serve:
    cd backend && cargo run --bin octo -- serve

# Start frontend dev server
dev:
    cd frontend && bun dev

# Install all dependencies and binaries
install:
    cd frontend && bun install
    cd backend && cargo install --path .
    cd fileserver && cargo install --path .

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
