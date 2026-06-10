# Oqto - AI Agent Workspace Platform

default:
    @just --list

# =============================================================================
# Build
# =============================================================================

# Build all components
build: agent-check-on-change build-backend build-frontend

# Build backend crates
build-backend:
    cd backend && cargo build --release -p oqto --bin oqto
    cd backend && cargo build --release -p oqto-runner --bin oqto-runner
    cd backend && cargo build --release -p oqto-files --bin oqto-files

# Build frontend
build-frontend:
    cd frontend && bun run build

# =============================================================================
# Lint & Format
# =============================================================================

# Run all linters
lint: lint-backend lint-frontend lint-rust-ai-guardrails lint-backend-crate-boundaries lint-no-legacy-history-authority

# Lint backend (clippy + fmt check)
lint-backend:
    cd backend && cargo clippy
    cd backend && cargo fmt --check

# Lint frontend
lint-frontend:
    cd frontend && bun run lint

# Install ast-grep (required for guardrails)
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

# Guardrail: enforce backend crate dependency direction
lint-backend-crate-boundaries:
    ./scripts/lint/backend-crate-boundaries.py

# Guardrail: prevent reintroduction of hstry-as-authority read paths
lint-no-legacy-history-authority:
    ./scripts/lint/no-legacy-history-authority.sh

# Validate dist/manifest.toml structure and asset references
lint-dist-manifest:
    ./scripts/lint/verify-dist-manifest.py --allow-missing-binaries

# Strict dist manifest validation (all referenced sources must exist)
lint-dist-manifest-strict:
    ./scripts/lint/verify-dist-manifest.py

# Sync canonical template/extension sources into dist/
dist-sync:
    ./scripts/dist/sync.sh

# Stage built binaries into dist/immutable/bin/ (use --build to compile first)
dist-stage-binaries *ARGS:
    ./scripts/dist/stage-binaries.sh {{ARGS}}

# Package dist payload as release tarball (+sha256)
dist-package version="dev" target="local":
    ./scripts/dist/package.sh {{version}} {{target}}

# =============================================================================
# Test
# =============================================================================

# Run all tests
test: agent-check-on-change test-backend test-frontend

# Test backend
test-backend:
    cd backend && cargo test

# Test frontend
test-frontend:
    cd frontend && bun run test

# Run oqto-sandbox hardening scenarios against the installed binary
test-sandbox *ARGS:
    ./scripts/sandbox/tests/run-all.sh {{ARGS}}

# =============================================================================
# Format / Check / Types
# =============================================================================

# Format all Rust code
fmt:
    cd backend && cargo fmt

# Generate TypeScript types from Rust structs
gen-types:
    cd backend && cargo test -p oqto export_typescript_bindings -- --nocapture
    cd frontend && bun run format:generated-types

# Check all Rust code compiles
check: agent-check-on-change
    cd backend && cargo check

# =============================================================================
# Dev Server
# =============================================================================

# Start backend server
serve:
    /usr/local/bin/oqto serve

# Start frontend dev server
dev: agent-check-on-change
    cd frontend && bun dev

# Fast reload: rebuild backend, install binaries, restart services
reload:
    ./scripts/fast-reload.sh

# Restart services with runner stream tracing enabled (systemd --user)
restart-debug trace_dir="/tmp/oqto-stream-traces":
    ./scripts/restart-debug.sh --trace-dir {{trace_dir}}

# Disable runner stream tracing and restart runner
restart-debug-off:
    ./scripts/restart-debug.sh --disable

# =============================================================================
# Install
# =============================================================================

# Install all dependencies and binaries
install-all:
    cd frontend && bun install
    cd backend/crates/oqto-browserd && bun install && bun run build
    cd backend && cargo install --path crates/oqto
    cd backend && cargo install --path crates/oqtoctl --bin oqtoctl
    cd backend && cargo install --path crates/oqto-runner --bin oqto-runner
    cd backend && cargo install --path crates/oqto-files
    cd ../hstry && cargo install --path crates/hstry-cli || echo "hstry build failed, skipping"

# Install a specific crate by name (e.g. just install oqto-browser)
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
            cd backend && cargo install --path crates/oqtoctl --bin oqtoctl
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
install-system:
    #!/usr/bin/env bash
    set -euo pipefail

    OQTO_ROOT="$(pwd)"
    sudo -v

    just install-all

    # Install sldr binaries (as current user, not sudo - avoids rustup issues with root)
    cd ../sldr && cargo install --path crates/sldr-cli && cargo install --path crates/sldr-server

    cd "$OQTO_ROOT"

    if [[ "$(uname -s)" != "Linux" ]]; then
      echo "install-system is Linux-only"
      exit 1
    fi

    sudo groupadd -f oqto || true
    sudo usermod -a -G oqto "$(id -un)" || true

    sudo install -Dm644 deploy/systemd/oqto-runner.service /usr/lib/systemd/user/oqto-runner.service
    sudo install -Dm644 deploy/systemd/hstry.service /usr/lib/systemd/user/hstry.service
    sudo install -Dm644 deploy/systemd/eavs.service /usr/lib/systemd/user/eavs.service
    sudo install -Dm644 deploy/systemd/oqto-runner.tmpfiles.conf /usr/lib/tmpfiles.d/oqto-runner.conf
    sudo systemd-tmpfiles --create /usr/lib/tmpfiles.d/oqto-runner.conf || true
    sudo systemctl daemon-reload || true

    sudo install -d -m 2770 -o "$(id -un)" -g oqto "/run/oqto/runner-sockets/$(id -un)" || true

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
      if [[ -e "$dst" ]] && cmp -s "$src" "$dst"; then
        continue
      fi
      sudo install -m 0755 "$src" "$dst"
    done

    # Install oqto-browserd daemon bundle
    sudo install -d -m 0755 /usr/local/lib/oqto-browserd
    sudo rsync -a --delete backend/crates/oqto-browserd/dist/ /usr/local/lib/oqto-browserd/dist/
    sudo rsync -a --delete backend/crates/oqto-browserd/node_modules/ /usr/local/lib/oqto-browserd/node_modules/
    sudo install -m 0644 backend/crates/oqto-browserd/package.json /usr/local/lib/oqto-browserd/package.json
    sudo install -d -m 0755 /usr/local/lib/oqto-browserd/bin
    sudo install -m 0755 backend/crates/oqto-browserd/bin/oqto-browserd.js /usr/local/lib/oqto-browserd/bin/oqto-browserd.js
    printf '#!/usr/bin/env bash\nexec node /usr/local/lib/oqto-browserd/dist/index.js "$@"\n' | sudo tee /usr/local/bin/oqto-browserd > /dev/null
    sudo chmod 0755 /usr/local/bin/oqto-browserd

    sudo loginctl enable-linger "$(id -un)" || true

# =============================================================================
# Dependencies
# =============================================================================

# Update external dependencies manifest from local repos and git tags
update-deps:
    #!/usr/bin/env bash
    set -euo pipefail

    ROOT="$(pwd)"
    MANIFEST="$ROOT/dependencies.toml"

    echo "Updating dependencies.toml..."

    OQTO_VERSION=$(grep -m1 '^version = ' "$ROOT/backend/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')
    sed -i '/^\[oqto\]$/,/^\[/{s/^\(version = \)"[^"]*"/\1"'"$OQTO_VERSION"'"/}' "$MANIFEST"
    echo "  oqto: $OQTO_VERSION"

    in_section=false
    while IFS= read -r line; do
      if [[ "$line" =~ ^\[byteowlz\]$ ]]; then
        in_section=true; continue
      elif [[ "$line" =~ ^\[.+\]$ ]]; then
        in_section=false; continue
      fi

      $in_section || continue
      [[ "$line" =~ ^[a-zA-Z] ]] || continue

      key=$(echo "$line" | sed 's/ *=.*//')
      old_val=$(echo "$line" | sed 's/.*= *"\([^"]*\)".*/\1/')

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

      version=""

      if [[ -f "$repo_dir/Cargo.toml" ]]; then
        version=$(grep -m1 '^version = ' "$repo_dir/Cargo.toml" | sed 's/version = "\(.*\)"/\1/' || true)
        if [[ -z "$version" ]]; then
          for member_dir in "$repo_dir/$key" "$repo_dir/$(echo "$key" | tr '[:upper:]' '[:lower:]')"; do
            if [[ -f "$member_dir/Cargo.toml" ]]; then
              version=$(grep -m1 '^version = ' "$member_dir/Cargo.toml" | sed 's/version = "\(.*\)"/\1/' || true)
              [[ -n "$version" ]] && break
            fi
          done
        fi
        if [[ -z "$version" ]]; then
          version=$(sed -n '/\[workspace.package\]/,/^\[/p' "$repo_dir/Cargo.toml" \
            | grep -m1 '^version = ' | sed 's/version = "\(.*\)"/\1/' || true)
        fi
      fi

      if [[ -z "$version" && -f "$repo_dir/go.mod" ]]; then
        version=$(grep -rh '[vV]ersion\s*=\s*"[0-9]' "$repo_dir"/*.go "$repo_dir"/cmd/ "$repo_dir"/internal/ 2>/dev/null \
          | grep -m1 -oP '"\K[0-9]+\.[0-9]+\.[0-9]+' || true)
        if [[ -z "$version" ]]; then
          version=$(cd "$repo_dir" && git describe --tags --abbrev=0 2>/dev/null | sed 's/^v//' || true)
        fi
      fi

      if [[ -z "$version" && -f "$repo_dir/pyproject.toml" ]]; then
        version=$(grep -m1 '^version = ' "$repo_dir/pyproject.toml" | sed 's/version = "\(.*\)"/\1/' || true)
      fi
      if [[ -z "$version" && -f "$repo_dir/package.json" ]]; then
        version=$(jq -r '.version // empty' "$repo_dir/package.json" 2>/dev/null || true)
      fi

      if [[ -n "$version" ]]; then
        sed -i "s|^\($key = \"\)[^\"]*\"|\1$version\"|" "$MANIFEST"
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
    FILTER="{{ARGS}}"

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
    BINS=()

    for key in "${!TOOLS[@]}"; do
      manifest_ver="${TOOLS[$key]}"
      if [[ -n "$FILTER" && "$key" != "$FILTER" ]]; then
        continue
      fi

      repo_dir=""
      for candidate in "$ROOT/../$key" "$ROOT/../$(echo "$key" | tr '[:upper:]' '[:lower:]')"; do
        [[ -d "$candidate" ]] && repo_dir="$candidate" && break
      done

      if [[ -z "$repo_dir" || ! -f "$repo_dir/Cargo.toml" ]]; then
        echo "SKIP $key: no sibling repo found"
        continue
      fi

      bin="$key"
      case "$key" in
        eaRS) bin="ears" ;;
        kokorox) bin="koko" ;;
        mailz) bin="mailz-cli" ;;
      esac

      if [[ -x "/usr/local/bin/$bin" ]]; then
        installed_ver=$(/usr/local/bin/$bin --version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
        [[ -z "$installed_ver" ]] && installed_ver=$(/usr/local/bin/$bin version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
      else
        installed_ver=$($bin --version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
        [[ -z "$installed_ver" ]] && installed_ver=$($bin version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
      fi

      if [[ "$installed_ver" == "$manifest_ver" && -z "$FILTER" ]]; then
        echo "OK   $key ($manifest_ver already installed)"
        continue
      fi

      install_path=""
      if grep -q '^\[workspace\]' "$repo_dir/Cargo.toml"; then
        lkey=$(echo "$key" | tr '[:upper:]' '[:lower:]')
        for crate_dir in "$repo_dir/crates/${lkey}-cli" "$repo_dir/crates/$lkey" "$repo_dir/${lkey}-cli" "$repo_dir/$lkey"; do
          if [[ -f "$crate_dir/Cargo.toml" && -f "$crate_dir/src/main.rs" ]]; then
            install_path="$crate_dir"
            break
          fi
        done
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
      BINS+=("$bin")
    done

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

    if [[ ${#NAMES[@]} -gt 0 ]]; then
      echo ""
      echo "Installing to /usr/local/bin..."
      for i in "${!NAMES[@]}"; do
        b="${BINS[$i]}"
        cargo_bin="$HOME/.cargo/bin/$b"
        if [[ -x "$cargo_bin" ]]; then
          sudo install -m 755 "$cargo_bin" "/usr/local/bin/$b"
          echo "  $b -> /usr/local/bin/$b"
        fi
      done
    fi

    VERIFY_FAIL=0
    if [[ ${#NAMES[@]} -gt 0 ]]; then
      echo ""
      echo "Verifying installed versions..."
      for i in "${!NAMES[@]}"; do
        b="${BINS[$i]}"
        k="${NAMES[$i]}"
        want="${TOOLS[$k]}"
        got=$(/usr/local/bin/$b --version 2>/dev/null | grep -oP '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
        if [[ "$got" == "$want" ]]; then
          echo "  $b: $got (ok)"
        else
          echo "  $b: expected $want, got ${got:-unknown} (MISMATCH)"
          VERIFY_FAIL=$((VERIFY_FAIL + 1))
        fi
      done
      if [[ $VERIFY_FAIL -gt 0 ]]; then
        echo ""
        echo "WARNING: $VERIFY_FAIL tool(s) did not match expected version."
      fi
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

    OQTO_MANIFEST=$(sed -n '/^\[oqto\]/,/^\[/{s/^version = "\(.*\)"/\1/p}' "$MANIFEST")
    OQTO_INSTALLED=$(grep -m1 '^version = ' backend/Cargo.toml | sed 's/version = "\(.*\)"/\1/')
    if [[ "$OQTO_MANIFEST" == "$OQTO_INSTALLED" ]]; then
      printf "%-12s %-12s %-12s %s\n" "oqto" "$OQTO_MANIFEST" "$OQTO_INSTALLED" "ok"
      OK=$((OK + 1))
    else
      printf "%-12s %-12s %-12s %s\n" "oqto" "$OQTO_MANIFEST" "$OQTO_INSTALLED" "MISMATCH"
      WARN=$((WARN + 1))
    fi

    in_section=false
    while IFS= read -r line; do
      if [[ "$line" =~ ^\[byteowlz\]$ ]]; then in_section=true; continue; fi
      if [[ "$line" =~ ^\[.+\]$ ]]; then in_section=false; continue; fi
      $in_section || continue
      [[ "$line" =~ ^[a-zA-Z] ]] || continue

      key=$(echo "$line" | sed 's/ *=.*//')
      manifest_ver=$(echo "$line" | sed 's/.*= *"\([^"]*\)".*/\1/')

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

    get_version() {
        local file="$1"
        grep -m1 '^version = ' "$file" 2>/dev/null | sed 's/version = "\(.*\)"/\1/' || echo ""
    }

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

        if [[ -f "$REPO_DIR/Cargo.toml" ]]; then
            LOCAL_VERSION=$(get_version "$REPO_DIR/Cargo.toml")
            if [[ -z "$LOCAL_VERSION" ]]; then
                FIRST_MEMBER=$(grep '^members' "$REPO_DIR/Cargo.toml" | head -1 | sed 's/.*\[\"\([^\"]*\).*/\1/')
                if [[ -n "$FIRST_MEMBER" && -f "$REPO_DIR/$FIRST_MEMBER/Cargo.toml" ]]; then
                    LOCAL_VERSION=$(get_version "$REPO_DIR/$FIRST_MEMBER/Cargo.toml")
                fi
            fi
            if [[ -z "$LOCAL_VERSION" ]]; then
                LOCAL_VERSION="unknown"
            fi
            echo "  Local: $LOCAL_VERSION"
            if REMOTE_TAG=$(git ls-remote --tags https://github.com/$repo_path 2>/dev/null | tail -1 | sed 's/.*refs\/tags\///' | sed 's/\^{}//'); then
                if [[ -n "$REMOTE_TAG" ]]; then
                    echo "  Remote: $REMOTE_TAG"
                    if [[ "$LOCAL_VERSION" != "unknown" ]]; then
                        LOCAL_NORMALIZED="${LOCAL_VERSION#v}"
                        REMOTE_NORMALIZED="${REMOTE_TAG#v}"
                        if [[ "$LOCAL_NORMALIZED" != "$REMOTE_NORMALIZED" ]]; then
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
            if REMOTE_TAG=$(git ls-remote --tags https://github.com/$repo_path 2>/dev/null | tail -1 | sed 's/.*refs\/tags\///' | sed 's/\^{}//'); then
                if [[ -n "$REMOTE_TAG" ]]; then
                    echo "  Remote: $REMOTE_TAG"
                fi
            fi
        fi
        echo ""
    done

    echo "Note: pi is installed from npm as @earendil-works/pi-coding-agent, sx has no tags yet"

# =============================================================================
# Version
# =============================================================================

# Bump version across all components
# Usage: just bump patch|minor|major|x.y.z
bump version:
    #!/usr/bin/env bash
    set -euo pipefail

    ROOT="$(git rev-parse --show-toplevel)"
    current=$(grep -m1 '^version = ' "$ROOT/backend/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')
    IFS='.' read -r major minor patch <<< "$current"

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
            new_version="{{version}}"
            ;;
    esac

    echo "Bumping $current -> $new_version"

    sed -i 's/^version = ".*"/version = "'"$new_version"'"/' "$ROOT/backend/Cargo.toml"
    sed -i '0,/^version = /s/^version = ".*"/version = "'"$new_version"'"/' "$ROOT/frontend/src-tauri/Cargo.toml"

    cd "$ROOT/backend" && cargo check --quiet 2>/dev/null || cargo generate-lockfile
    echo "Cargo.lock regenerated"

    cd "$ROOT/frontend" && bun pm pkg set version="$new_version"

    jq --arg v "$new_version" '.version = $v' "$ROOT/frontend/src-tauri/tauri.conf.json" > "$ROOT/frontend/src-tauri/tauri.conf.json.tmp" \
        && mv "$ROOT/frontend/src-tauri/tauri.conf.json.tmp" "$ROOT/frontend/src-tauri/tauri.conf.json"

    echo "Bumped all components to $new_version"

# Release: bump version, commit, and tag
release version:
    #!/usr/bin/env bash
    set -euo pipefail

    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "Error: uncommitted changes exist. Commit or stash them first."
        exit 1
    fi

    current=$(grep -m1 '^version = ' backend/Cargo.toml | sed 's/version = "\(.*\)"/\1/')
    IFS='.' read -r major minor patch <<< "$current"

    case "{{version}}" in
        patch) new_version="$major.$minor.$((patch + 1))" ;;
        minor) new_version="$major.$((minor + 1)).0" ;;
        major) new_version="$((major + 1)).0.0" ;;
        *) new_version="{{version}}" ;;
    esac

    just bump {{version}}

    git add -A
    git commit -m "release: v$new_version"
    git tag -a "v$new_version" -m "Release v$new_version"

    echo ""
    echo "Released v$new_version"
    echo "Run 'git push && git push --tags' to publish"

# =============================================================================
# Git
# =============================================================================

# Git add everything except uploads folder
add:
    git add --all -- ':!uploads/'

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

# =============================================================================
# Deployment
# =============================================================================

# Deploy to all configured hosts (build + upload + restart)
deploy *ARGS:
    ./scripts/deploy.sh {{ARGS}}

# Deploy to a specific host only
deploy-host name *ARGS:
    ./scripts/deploy.sh --host {{name}} {{ARGS}}

# Deploy to a specific host with runner stream tracing enabled
deploy-host-debug name trace_dir="/tmp/oqto-stream-traces" *ARGS:
    ./scripts/deploy.sh --host {{name}} --trace-streams --trace-dir {{trace_dir}} {{ARGS}}

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

# End-to-end retry/error trace capture via agent-browser + frontend console + runner traces
trace-retry-e2e *ARGS:
    ./scripts/debug-e2e-retry-trace.sh {{ARGS}}

# =============================================================================
# Admin
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
    bun install -g @earendil-works/pi-coding-agent@latest
    source scripts/setup/05-install-core.sh
    ensure_bun_and_pi_global
    echo "Restarting oqto-runner..."
    systemctl --user restart oqto-runner
    echo "Done. Pi version: $(/usr/local/bin/pi --version 2>/dev/null)"

# =============================================================================
# Agent Quality Gate
# =============================================================================

# Automatic non-daemon quality gate for agent workflows.
# Runs only when git working tree changed since the last successful check.
agent-check-on-change profile="quick":
    ./scripts/agent-check-on-change.sh {{profile}}
