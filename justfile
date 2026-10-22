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
    ./scripts/lint/verify-dist-manifest.py --allow-missing-binaries --allow-missing-extensions --allow-missing-frontend --allow-missing-runtime

# Strict dist manifest validation (all referenced sources must exist)
lint-dist-manifest-strict:
    ./scripts/lint/verify-dist-manifest.py

# Sync canonical template/extension sources into dist/
dist-sync:
    ./scripts/dist/sync.sh

# Stage built binaries into dist/immutable/bin/ (use --build to compile first)
dist-stage-binaries *ARGS:
    ./scripts/dist/stage-binaries.sh {{ARGS}}

# Ship the checksum-qualified official Pi1 archive with the release.
dist-stage-pi-runtime:
    ./scripts/dist/stage-pi-runtime.sh

# Prepare an explicit new Node/npm Pi runtime candidate (no live promotion).
dist-stage-pi-node-runtime output:
    ./scripts/dist/stage-pi-node-runtime.sh '{{output}}'

# Locked acquisition and preserve-first staging contracts (no downloads).
test-pi-node-runtime:
    python3 -B scripts/tests/test-pi-node-runtime.py

# Stage the built frontend into dist/immutable/frontend/ (use --build to build first)
dist-stage-frontend *ARGS:
    ./scripts/dist/stage-frontend.sh {{ARGS}}

# Package dist payload as release tarball (+sha256)
# Package the declared Cargo version for this host; refuses dirty worktrees.
dist-package version="" target="":
    ./scripts/dist/package.sh "{{version}}" "{{target}}"

# =============================================================================
# Test
# =============================================================================

# Run all tests
test: agent-check-on-change test-backend test-frontend

# Test backend
test-backend:
    cd backend && cargo test

# Encrypted provider foundation only; not credential broker/UI integration.
# Optimize test dependencies so production-strength scrypt remains practical.
test-credential-provider:
    cd backend && cargo fmt -p oqto-secrets --check
    cd backend && CARGO_PROFILE_TEST_OPT_LEVEL=1 cargo test --locked -p oqto-secrets
    cd backend && cargo clippy --locked -p oqto-secrets --all-targets -- -D warnings
    ./scripts/lint/backend-crate-boundaries.py

# Regenerate sandbox.schema.json from the Rust types.
schema-update:
    cd backend && cargo run -q -p oqto-sandbox --example emit_schema > crates/oqto/examples/sandbox.schema.json
    @echo "wrote backend/crates/oqto/examples/sandbox.schema.json"

# Sandbox config gate: schema matches the types, shipped configs still parse.
test-sandbox-schema:
    cd backend && cargo test -p oqto-sandbox --test schema

# Experimental source-only Durable runtime. Never installs or touches live profiles.
test-oqt-core: test-sandbox-schema
    python3 tools/oqt/preparation_lock_test.py
    python3 tools/oqt/profile_namespace_test.py
    cd backend && cargo fmt -p oq-permissions -p oqto-sandbox --check
    cd backend && cargo test --locked -p oq-permissions --lib --test lifecycle --test ceiling --test operator_receipts
    cd backend && cargo test --locked -p oqto-sandbox --lib --test guard_config
    cd backend && cargo clippy --locked -p oq-permissions -p oqto-sandbox --all-targets -- -D warnings
    node --test --test-concurrency=1 tools/oqt/input-leases.test.mjs tools/oqt/guard-wire.test.mjs tools/oqt/classic-import.test.mjs tools/oqt/classic-source.test.mjs tools/oqt/classic-client.test.mjs tools/oqt/classic-cli.test.mjs

# Owned derivative uses its pinned SDK's Biome, not the separate frontend version.
test-oqt-lint PI_SOURCE:
    "{{PI_SOURCE}}/node_modules/.bin/biome" check tools/oqt/input-leases.ts tools/oqt/input-leases.test.mjs tools/oqt/classic-*.ts tools/oqt/classic-*.mjs tools/oqt/session-*.ts tools/oqt/session-*.mjs tools/oqt/operator-*.ts tools/oqt/operator-*.mjs tools/oqt/guard*.ts tools/oqt/guard*.mjs tools/oqt/launcher.ts tools/oqt/coding-worker.ts tools/oqt/plugin/src/guard-client.ts tools/oqt/plugin/src/tui.ts

test-oqt-typecheck PI_SOURCE:
    bash tools/oqt/typecheck.sh "{{PI_SOURCE}}"

# Native persistence/recovery qualification; no provider or owner-profile access.
test-oqt-operator-context PI_SOURCE:
    OQT_PI_SOURCE="{{PI_SOURCE}}" timeout 40 node --import "{{PI_SOURCE}}/packages/coding-agent/src/experimental/source-resolver.ts" --test --test-concurrency=1 tools/oqt/operator-context-native.test.mjs tools/oqt/operator-receipts.test.mjs

# Developer-only standalone runtime snapshot; no platform services/releases.
install-oqt:
    bash tools/oqt/install-oqt.sh

test-oqt-install:
    timeout 90 python3 tools/oqt/install_runtime_test.py
    timeout 30 python3 tools/oqt/installed_state_test.py

# Real native import storage and bounded host transport; disposable fixtures only.
test-oqt-import-native PI_SOURCE:
    OQT_PI_SOURCE="{{PI_SOURCE}}" timeout 90 node --import "{{PI_SOURCE}}/packages/coding-agent/src/experimental/source-resolver.ts" --test --test-concurrency=1 tools/oqt/classic-import-native.test.mjs tools/oqt/classic-import-host.test.mjs

test-oqt-forks PI_SOURCE:
    OQT_PI_SOURCE="{{PI_SOURCE}}" timeout 90 node --import "{{PI_SOURCE}}/packages/coding-agent/src/experimental/source-resolver.ts" --test --test-concurrency=1 tools/oqt/session-*.test.mjs

test-oqt-forks-tui:
    OQT_HOST_IDLE_SECONDS=1 timeout --kill-after=5s 260s python3 tools/oqt/session_tree_tui_test.py

test-oqt-import-tui:
    OQT_HOST_IDLE_SECONDS=1 timeout --kill-after=5s 300s python3 tools/oqt/classic_import_tui_test.py

test-oqt-hands PI_SOURCE DAEMON BPF:
    bash tools/oqt/test-hands.sh "{{PI_SOURCE}}" "{{DAEMON}}" "{{BPF}}"

test-oqt-ui:
    OQT_HOST_IDLE_SECONDS=1 timeout --kill-after=5s 160s python3 tools/oqt/tui_test.py

test-oqt-coding:
    OQT_HOST_IDLE_SECONDS=1 timeout --kill-after=5s 100s python3 tools/oqt/coding_tui_test.py

# Real TUI plus provider-request capture across client reopen; disposable profile.
test-oqt-bang:
    OQT_HOST_IDLE_SECONDS=1 timeout --kill-after=5s 180s python3 tools/oqt/bang_tui_test.py

test-oqt-operator-recovery:
    OQT_HOST_IDLE_SECONDS=1 timeout --kill-after=5s 210s python3 tools/oqt/operator_recovery_tui_test.py

test-oqt-sandbox-config:
    OQT_HOST_IDLE_SECONDS=1 timeout --kill-after=5s 180s python3 tools/oqt/sandbox_config_tui_test.py

test-oqt-recovery:
    OQT_HOST_IDLE_SECONDS=1 timeout --kill-after=5s 260s python3 tools/oqt/recovery_tui_test.py

test-oqt-multi:
    OQT_HOST_IDLE_SECONDS=1 timeout --kill-after=5s 200s python3 tools/oqt/multiclient_tui_test.py

# Sandbox containment gate. OQTO_REQUIRE_SANDBOX=1 makes a missing sandbox a
# failure rather than a skip.
test-containment:
    cd backend && OQTO_REQUIRE_SANDBOX=1 cargo test -p oqto-sandbox --test containment

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
    ./scripts/dev/fast-reload.sh

# Restart services with runner stream tracing enabled (systemd --user)
restart-debug trace_dir="/tmp/oqto-stream-traces":
    ./scripts/dev/restart-debug.sh --trace-dir {{trace_dir}}

# Disable runner stream tracing and restart runner
restart-debug-off:
    ./scripts/dev/restart-debug.sh --disable

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
    sudo install -Dm644 deploy/systemd/eavs.service /usr/lib/systemd/user/eavs.service
    sudo install -Dm644 deploy/systemd/oqto-runner.tmpfiles.conf /usr/lib/tmpfiles.d/oqto-runner.conf
    sudo systemd-tmpfiles --create /usr/lib/tmpfiles.d/oqto-runner.conf || true
    sudo systemctl daemon-reload || true

    sudo install -d -m 2770 -o "$(id -un)" -g oqto "/run/oqto/runner-sockets/$(id -un)" || true

    for bin in trx mmry agntz skdlr oqto oqto-runner oqto-files sldr sldr-server eavs; do
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

# Discover official stable Pi without changing accepted pins or live state
check-pi-stable:
    python3 -B scripts/release/pi-update.py

# Private real-sandbox RPC/provider/cold-restore proof; never touches live stores.
test-sandboxed-bundled-pi runner runtime:
    python3 -B scripts/dist/test-sandboxed-bundled-pi.py "{{runner}}" "{{runtime}}"

# Prevent local speech isolation headers drifting out of generated Caddy profiles.
test-speech-isolation:
    python3 -B scripts/tests/test-speech-isolation.py

# Combined immutable Node runtime and runner identity regression gates
_test-node-canary:
    python3 -B scripts/tests/test-node-cli-links.py
    python3 -B scripts/tests/test-pi-node-runtime.py
    python3 -B scripts/tests/test-agent-check-root.py
    cd backend && cargo test -p oqto-runner -p oqto-setup -p oqto-provisioning
    cd backend && cargo test -p oqto-runner --test runtime_guards_cli wrong_uid_native_launch_fails_before_state_or_workers -- --ignored --exact

# Regression gates for isolated exact-pin candidate preparation
_test-pi-updates:
    python3 -B scripts/tests/test-pi-update.py
    python3 -B scripts/tests/test-pi-node-runtime.py
    bash -n scripts/dist/stage-pi-node-runtime.sh scripts/dist/test-node-pi-cpu.sh scripts/dist/test-node-pi-runner.sh

# Legacy standalone smoke only; not Node/npm release acceptance
check-agent-updates:
    ./scripts/dist/check-agent-runtime.sh

# Test latest Pi and update the Git bootstrap lock (manual release maintenance)
promote-agent-update:
    ./scripts/dist/check-agent-runtime.sh --update-lock

# Rootless local channel: verify and atomically promote latest, without Git changes
update-pi-local:
    ./scripts/dist/check-agent-runtime.sh --install-user
    systemctl --user try-restart oqto-runner

# Exercise the deterministic runtime installer and read-only personal onboarding inventory
_test-pi-runtime:
    ./scripts/dist/test-pi-runtime.sh
    python3 -B scripts/dist/test-personal-pi-preflight.py

# Re-verify installer behavior plus the currently pinned Pi runtime/checksums
verify-agent-runtime: _test-pi-runtime
    ./scripts/dist/check-agent-runtime.sh --verify-current

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

# Acquire PREBUILT dependency binaries from GitHub releases and install them
# (ADR-0018 / vemr.9): the manifest-driven, checksum-verified path via
# `oqto-setup acquire --install-bin`. Additive alternative to `install-deps`
# (which builds from sibling source). Needs sudo for /usr/local/bin.
acquire-deps arch="x86-64":
    #!/usr/bin/env bash
    set -euo pipefail
    SETUP="$(command -v oqto-setup || true)"
    if [ -z "$SETUP" ]; then
        echo "building oqto-setup..."
        (cd backend && cargo build -q -p oqto-setup)
        SETUP="$(pwd)/backend/target/debug/oqto-setup"
    fi
    echo "Acquiring + installing prebuilt deps (arch={{arch}}) -> /usr/local/bin"
    # The installer privately stages and checks the exact bytes it extracts.
    sudo "$SETUP" acquire \
        --manifest "$(pwd)/dependencies.toml" \
        --arch "{{arch}}" \
        --install-bin /usr/local/bin

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
        ["byteowlz/mmry"]="mmry"
        ["byteowlz/trx"]="trx"
        ["byteowlz/agntz"]="agntz"
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

    echo "Note: pi uses the checksummed official standalone release; sx has no tags yet"

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

    [[ "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Invalid version: $new_version" >&2; exit 1; }
    echo "Bumping $current -> $new_version"
    python3 "$ROOT/scripts/release/sync-platform-pin.py" "$new_version"

    sed -i 's/^version = ".*"/version = "'"$new_version"'"/' "$ROOT/backend/Cargo.toml"
    sed -i '0,/^version = /s/^version = ".*"/version = "'"$new_version"'"/' "$ROOT/frontend/src-tauri/Cargo.toml"

    cd "$ROOT/backend" && cargo check --quiet 2>/dev/null || cargo generate-lockfile
    echo "Cargo.lock regenerated"

    cd "$ROOT/frontend" && bun pm pkg set version="$new_version"

    # Edit only the version value: re-serializing (jq) breaks the Biome format gate.
    python3 "$ROOT/scripts/release/set-tauri-version.py" "$ROOT/frontend/src-tauri/tauri.conf.json" "$new_version"

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

# Deploy to all configured hosts: build a clean versioned release (or use
# --artifact/--checksum) and activate it through the oqto-setup transaction.
deploy *ARGS:
    ./scripts/deploy.sh {{ARGS}}

# Deploy to a specific host only
deploy-host name *ARGS:
    ./scripts/deploy.sh --host {{name}} {{ARGS}}

# Protected owner-correct oqto-log backfill only; binaries are not touched
deploy-backfill name *ARGS:
    ./scripts/deploy.sh --host {{name}} --backfill-only {{ARGS}}

# Offline publication gate tests (no build, download, or deployment)
test-release-publication-gates: test-release-tag-version test-release-assets

# Verify tag matches Cargo, dependency pin, and frontend package versions
test-release-tag-version:
    python3 -B scripts/tests/test-release-tag-version.py

# Reject incomplete or unsafe canonical Linux full release artifacts
test-release-assets:
    python3 -B scripts/tests/test-release-assets.py

# Managed asset recognition, preservation, rollback and repeat-run producer gates
# (stdlib only; no root, real profiles, services or provider credentials)
test-managed-pi-extensions:
    python3 -B scripts/tests/test-managed-pi-extensions.py
    bash scripts/dist/test-sync-agent-runtime-extensions.sh
    bash scripts/dist/test-sync-agent-runtime-extensions.sh --skip-pi --non-interactive
    bash scripts/dist/test-pi-todo-supersession.sh

# Actual daemon early-exit, broken-stdin, cancellation and no-fallback regressions
test-pi-startup-readiness:
    cd backend && CARGO_BUILD_JOBS=2 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked -p oqto-runner --test pi_startup_readiness -- --test-threads=1

# Actual native autoload + complete extension closure + loopback mock-provider turn
# Explicit binaries/assets and a fresh output directory; no production state
test-pi-extension-supersession binary source output:
    python3 -B scripts/dist/test-pi-extension-supersession.py --binary {{quote(binary)}} --source {{quote(source)}} --legacy-source scripts/tests/fixtures/pi-oqto-todos --output {{quote(output)}}

# Deploy driver, transaction, provenance and bootstrap regressions (no host changes)
test-deploy-mode: test-managed-pi-extensions
    python3 -B scripts/tests/test-deploy-sudo.py
    ./scripts/tests/deploy-mode-test.sh
    ./scripts/tests/test-deploy-private-staging.sh
    ./scripts/tests/test-deploy-bootstrap-invocation.sh
    python3 -B scripts/tests/test-verified-setup-bootstrap.py
    python3 -B scripts/tests/test-deploy-transaction.py
    python3 -B scripts/tests/test-release-provenance.py
    ./scripts/tests/test-stage-pi-runtime-idempotent.sh

# Offline install/update matrix preflight regression checks (no root or host changes)
test-install-matrix:
    ./scripts/e2e/tests/test-install-update-matrix.sh

# Synthetic external-TUI transport proof; never scans or attaches to real sessions
test-pi-tui-rpc:
    cd backend && cargo fmt --all --check
    cd backend && cargo test --locked -p oqto-runner --lib tui_rpc
    cd backend && cargo clippy --locked -p oqto-runner --all-targets -- -D warnings

# Runner-only manifest is a fail-closed candidate, not an install-ready artifact
test-runner-only-manifest:
    python3 -B scripts/lint/test_runner_only_manifest.py

# Installed plan/doctor must never compile checkout sources or accept stale binaries
test-setup-installed-doctor:
    ./scripts/tests/test-setup-installed-doctor.sh

# Release dependency pin must equal the declared Cargo workspace version
test-release-version-sync:
    python3 -B scripts/tests/test-sync-platform-pin.py
    python3 scripts/tests/test-release-version-sync.py
    python3 -B scripts/tests/test-set-tauri-version.py

# Dist sources resolve to pinned refs, preserving dirty or unknown local caches
test-dist-sync-pins:
    ./scripts/tests/test-dist-sync-pins.sh

# Show what deploy would do without doing it
deploy-dry-run *ARGS:
    ./scripts/deploy.sh --dry-run {{ARGS}}

# End-to-end retry/error trace capture via agent-browser + frontend console + runner traces
trace-retry-e2e *ARGS:
    ./scripts/dev/debug-e2e-retry-trace.sh {{ARGS}}

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

# Alias for the rootless local update channel
update-pi: update-pi-local

# Component enrollment TLS proof only: not a running host service, Pi, Files/PTY or phone acceptance.
test-personal-authority:
    cd backend && cargo fmt -p oqto-personal -p oqto-personal-wire -- --check
    cd backend && cargo test --locked -p oqto-personal -p oqto-personal-wire --all-features
    cd backend && cargo clippy --locked -p oqto-personal -p oqto-personal-wire --all-targets --all-features -- -D warnings

# One-command pairing: strict default-feature gate + loopback process and PTY proofs.
test-pair-wizard:
    cd backend && cargo fmt -p oqto -p oqto-runner -p oqto-personal -- --check
    cd backend && cargo clippy --locked -p oqto -p oqto-runner -p oqto-personal --all-targets -- -D warnings
    cd backend && cargo test --locked -p oqto-personal owner::tests::
    cd backend && cargo test --locked -p oqto -p oqto-runner --test pair_cli --test pair_wizard --test pair_wizard_transport -- --test-threads=1
    cd backend && cargo test --locked -p oqto-runner --lib pair_cli::
    python3 scripts/tests/test-runner-pair-wizard.py --binary "$(cd backend && cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/debug/oqto"

# Disposable process/HTTPS proofs, including new test code in the strict gate.
# Uses deployed/default Runner features; optional Iroh qualification is separate.
test-personal-host:
    cd backend && cargo fmt -p oqto-runner -p oqto-history -- --check
    cd backend && cargo clippy --locked -p oqto-runner -p oqto-history --all-targets -- -D warnings
    cd backend && cargo test --locked -p oqto-history --lib
    cd backend && cargo test --locked -p oqto-runner --lib -- --test-threads=1
    cd backend && cargo test --locked -p oqto-runner --test personal_host

# =============================================================================
# Agent Quality Gate
# =============================================================================

# Automatic non-daemon quality gate for agent workflows.
# Runs only when git working tree changed since the last successful check.
agent-check-on-change profile="quick":
    ./scripts/dev/agent-check-on-change.sh {{profile}}

# Publish Oqto's setup skill to the canonical byteowlz skills repository.
sync-skills:
    #!/usr/bin/env bash
    set -euo pipefail
    target="${SKILLISSUES:-$HOME/byteowlz/skillissues}"
    test -d "$target/skills" || { echo "skillissues repo not found: $target" >&2; exit 1; }
    rm -rf "$target/skills/oqto-setup"
    cp -a skills/oqto-setup "$target/skills/"
    just --justfile "$target/Justfile" update-readme
    git -C "$target" add skills/oqto-setup README.md
    if [[ -n "$(git -C "$target" status --porcelain -- skills/oqto-setup README.md)" ]]; then
        git -C "$target" commit -m "skills/oqto-setup: sync from oqto_refactor" -- skills/oqto-setup README.md
    else
        echo "oqto-setup is already up to date"
    fi
