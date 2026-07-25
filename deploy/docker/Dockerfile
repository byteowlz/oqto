# =============================================================================
# Oqto All-in-One Dockerfile
# =============================================================================
#
# Monolithic image containing the complete Oqto platform:
#   - oqto backend (API + WebSocket server)
#   - oqto-runner (agent process manager)
#   - oqto-files (file server)
#   - pi-bridge, oqtoctl, oqto-sandbox
#   - eavs (LLM proxy)
#   - Frontend (static, served by caddy)
#   - Pi (AI agent runtime)
#   - All agent tools (agntz, mmry, tmpltr, sldr, ignr, scrpr, sx)
#   - Shell tools (tmux, rg, fd, fzf, zsh, neovim, yazi, jq, etc.)
#   - caddy (reverse proxy, serves frontend + proxies API)
#
# Build from repo root:
#   docker build -f deploy/docker/Dockerfile -t oqto:latest .
#
# Run:
#   docker run -d -p 8080:8080 \
#     -e ANTHROPIC_API_KEY=sk-ant-... \
#     -v oqto-data:/data \
#     oqto:latest
#
# =============================================================================

# ---------------------------------------------------------------------------
# Stage 1: Download ALL pre-built binaries
# ---------------------------------------------------------------------------
#
# Zero compilation. Everything comes from GitHub releases:
#   - oqto:            oqto, oqtoctl, oqto-runner, oqto-files, pi-bridge, etc.
#   - oqto frontend:   pre-built static assets (dist/)
#   - byteowlz tools:  eavs, agntz, mmry, tmpltr, sldr, ignr, trx, scrpr, sx
#   - external tools:   typst, caddy, yazi, starship
#
# Naming conventions:
#   Rust:  {repo}-{tag}-{target}.tar.gz       (e.g. mmry-v0.5.10-x86_64-unknown-linux-gnu.tar.gz)
#   Go:    {repo}_{OS}_{arch}.tar.gz           (e.g. sx_Linux_x86_64.tar.gz)
#   oqto:  oqto-{tag}-{target}.tar.gz         (binaries in bin/ subdir)
#          oqto-frontend-{tag}.tar.gz          (static assets in dist/)
#
FROM ubuntu:24.04 AS downloader
# Note: downloader uses plain ubuntu (no systemd needed here)

ARG TARGETARCH

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates xz-utils unzip \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /out
RUN mkdir -p /out/bin /out/frontend /out/lib

SHELL ["/bin/bash", "-c"]

RUN set -eux; \
    \
    # --- Platform detection --- \
    case "${TARGETARCH}" in \
      amd64) RUST_TARGET="x86_64-unknown-linux-gnu"; GO_OS="Linux"; GO_ARCH="x86_64"; CADDY_ARCH="amd64"; TYPST_TARGET="x86_64-unknown-linux-musl"; YAZI_TARGET="x86_64-unknown-linux-gnu" ;; \
      arm64) RUST_TARGET="aarch64-unknown-linux-gnu"; GO_OS="Linux"; GO_ARCH="arm64"; CADDY_ARCH="arm64"; TYPST_TARGET="aarch64-unknown-linux-musl"; YAZI_TARGET="aarch64-unknown-linux-gnu" ;; \
      *) echo "Unsupported arch: ${TARGETARCH}" && exit 1 ;; \
    esac; \
    \
    GH="https://github.com/byteowlz"; \
    tag() { curl -fsS -o /dev/null -w '%{redirect_url}' "$1/releases/latest" | grep -o '[^/]*$'; }; \
    \
    # ================================================================== \
    # Oqto platform \
    # Tarball: oqto-{tag}-{target}.tar.gz with bin/ subdirectory \
    # ================================================================== \
    echo "--- Downloading oqto ---"; \
    TAG=$(tag "$GH/oqto"); \
    curl -fsSL "$GH/oqto/releases/download/${TAG}/oqto-${TAG}-${RUST_TARGET}.tar.gz" \
      | tar xz --strip-components=1 -C /tmp/ \
    && cp /tmp/bin/* /out/bin/ \
    && if [ -d /tmp/lib/oqto-browserd ]; then cp -r /tmp/lib/oqto-browserd /out/lib/; fi; \
    \
    echo "--- Downloading oqto frontend ---"; \
    curl -fsSL "$GH/oqto/releases/download/${TAG}/oqto-frontend-${TAG}.tar.gz" \
      | tar xz --strip-components=2 -C /out/frontend/; \
    \
    # ================================================================== \
    # Byteowlz Rust tools (tarballs contain binaries at root level) \
    # ================================================================== \
    for repo in eavs agntz mmry tmpltr sldr ignr trx; do \
      echo "--- Downloading $repo ---"; \
      TAG=$(tag "$GH/$repo"); \
      curl -fsSL "$GH/$repo/releases/download/${TAG}/${repo}-${TAG}-${RUST_TARGET}.tar.gz" \
        | tar xz -C /out/bin/ \
      || echo "WARN: $repo download failed (may not have ${RUST_TARGET} build)"; \
    done; \
    \
    # ================================================================== \
    # Byteowlz Go tools (goreleaser naming: {repo}_{OS}_{arch}.tar.gz) \
    # ================================================================== \
    for repo in scrpr sx; do \
      echo "--- Downloading $repo ---"; \
      TAG=$(tag "$GH/$repo"); \
      curl -fsSL "$GH/$repo/releases/download/${TAG}/${repo}_${GO_OS}_${GO_ARCH}.tar.gz" \
        | tar xz -C /out/bin/ \
      || echo "WARN: $repo download failed"; \
    done; \
    \
    # ================================================================== \
    # External tools \
    # ================================================================== \
    echo "--- Downloading typst ---"; \
    curl -fsSL "https://github.com/typst/typst/releases/latest/download/typst-${TYPST_TARGET}.tar.xz" \
      | tar -xJ --strip-components=1 -C /out/bin/; \
    \
    echo "--- Downloading caddy ---"; \
    curl -fsSL "https://caddyserver.com/api/download?os=linux&arch=${CADDY_ARCH}" \
      -o /out/bin/caddy; \
    \
    echo "--- Downloading yazi ---"; \
    YAZI_TAG=$(curl -fsSL "https://api.github.com/repos/sxyazi/yazi/releases/latest" \
      | grep '"tag_name"' | head -1 | sed 's/.*"\([^"]*\)".*/\1/'); \
    curl -fsSL "https://github.com/sxyazi/yazi/releases/download/${YAZI_TAG}/yazi-${YAZI_TARGET}.zip" \
      -o /tmp/yazi.zip \
    && unzip -o /tmp/yazi.zip -d /tmp/yazi \
    && find /tmp/yazi -name 'yazi' -type f -exec cp {} /out/bin/yazi \; \
    || echo "WARN: yazi download failed"; \
    \
    echo "--- Downloading starship ---"; \
    curl -fsSL https://starship.rs/install.sh | sh -s -- --yes --bin-dir /out/bin/ \
    || echo "WARN: starship download failed"; \
    \
    echo "--- Downloading ttyd ---"; \
    case "${TARGETARCH}" in \
      amd64) TTYD_ARCH="x86_64" ;; \
      arm64) TTYD_ARCH="aarch64" ;; \
    esac; \
    curl -fsSL "https://github.com/tsl0922/ttyd/releases/latest/download/ttyd.${TTYD_ARCH}" \
      -o /out/bin/ttyd \
    || echo "WARN: ttyd download failed"; \
    \
    # ================================================================== \
    # Cleanup and verify \
    # ================================================================== \
    rm -f /out/bin/LICENSE* /out/bin/README* /out/bin/CHANGELOG* /out/bin/checksums* 2>/dev/null || true; \
    chmod +x /out/bin/* 2>/dev/null || true; \
    echo "=== Downloaded binaries ==="; ls -la /out/bin/; \
    echo "=== Frontend files ==="; ls /out/frontend/ | head -10; \
    \
    # ================================================================== \
    # Verify required binaries exist \
    # ================================================================== \
    MISSING=""; \
    for bin in oqto oqtoctl oqto-runner oqto-browser oqto-files pi-bridge eavs caddy ttyd; do \
      if [ ! -f "/out/bin/$bin" ]; then MISSING="$MISSING $bin"; fi; \
    done; \
    if [ -n "$MISSING" ]; then \
      echo "ERROR: Required binaries missing:$MISSING"; exit 1; \
    fi; \
    echo "=== All required binaries present ==="

# ---------------------------------------------------------------------------
# Stage 2: Final image
# ---------------------------------------------------------------------------
FROM ubuntu:24.04

LABEL org.opencontainers.image.title="oqto" \
      org.opencontainers.image.description="Oqto AI Agent Workspace Platform - All-in-One" \
      org.opencontainers.image.source="https://github.com/byteowlz/oqto"

# Core system packages
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Shell and terminal (ttyd installed from GitHub releases)
    zsh tmux \
    # Search and navigation
    ripgrep fd-find fzf \
    # Editors
    neovim \
    # Utilities
    jq git curl wget unzip openssh-client sudo ca-certificates \
    # Media (for tmpltr, agent tools)
    imagemagick ffmpeg poppler-utils \
    # Python (for uv, agent scripts)
    python3 python3-venv \
    # Fonts (for tmpltr PDF rendering)
    fonts-liberation fonts-noto fonts-noto-color-emoji fonts-dejavu \
    fonts-roboto fonts-inter \
    # Misc
    procps tini locales \
    && rm -rf /var/lib/apt/lists/* \
    # Generate locale
    && locale-gen en_US.UTF-8 \
    # fd-find -> fd symlink (Ubuntu installs as fdfind)
    && ln -sf "$(which fdfind)" /usr/local/bin/fd

# Install bun (JS runtime for Pi and other tools)
RUN curl -fsSL https://bun.sh/install | bash \
    && mv /root/.bun/bin/bun /usr/local/bin/bun \
    && rm -rf /root/.bun

# Install uv (Python package manager)
RUN curl -fsSL https://astral.sh/uv/install.sh | sh \
    && mv /root/.local/bin/uv /usr/local/bin/uv \
    && mv /root/.local/bin/uvx /usr/local/bin/uvx 2>/dev/null || true \
    && rm -rf /root/.local

# Install zoxide (smart cd)
RUN curl -fsSL https://raw.githubusercontent.com/ajeetdsouza/zoxide/main/install.sh | sh

# Install the official checksummed standalone Pi runtime. Agent-browser still
# uses Bun, but cannot influence Pi's internal dependency graph.
ARG TARGETARCH
COPY dependencies.toml /tmp/pi-dependencies.toml
RUN version="$(sed -n '/^\[external\]/,/^\[/{s/^pi *= *"\([^"]*\)".*/\1/p}' /tmp/pi-dependencies.toml | head -1)" \
    && case "$TARGETARCH" in \
      amd64|"") asset=pi-linux-x64.tar.gz; key=pi-linux-x64-sha256 ;; \
      arm64) asset=pi-linux-arm64.tar.gz; key=pi-linux-arm64-sha256 ;; \
      *) exit 64 ;; \
    esac \
    && sha="$(sed -n '/^\[external\]/,/^\[/{s/^'"$key"' *= *"\([^"]*\)".*/\1/p}' /tmp/pi-dependencies.toml | head -1)" \
    && test -n "$version" && test -n "$sha" \
    && curl -fsSL --retry 3 \
      "https://github.com/earendil-works/pi/releases/download/v${version}/${asset}" \
      -o /tmp/pi.tar.gz \
    && echo "$sha  /tmp/pi.tar.gz" | sha256sum --check --strict \
    && tar -xzf /tmp/pi.tar.gz -C /tmp \
    && install -m 0755 /tmp/pi/pi /usr/local/bin/pi \
    && bun install -g agent-browser \
    && test -f /root/.bun/bin/agent-browser \
    && cp /root/.bun/bin/agent-browser /usr/local/bin/agent-browser \
    && chmod 755 /usr/local/bin/agent-browser \
    && printf '#!/bin/sh\nexec /usr/local/bin/agent-browser "$@"\n' > /usr/local/bin/oqto-browser \
    && chmod 755 /usr/local/bin/oqto-browser \
    && rm -rf /root/.bun/install /tmp/pi /tmp/pi.tar.gz /tmp/pi-dependencies.toml

# --- Copy binaries from downloader stage ---
COPY --from=downloader /out/bin/ /usr/local/bin/

# Frontend static assets
COPY --from=downloader /out/frontend/ /usr/local/share/oqto/frontend/

# Optional oqto-browserd runtime assets (from oqto release tarball)
COPY --from=downloader /out/lib/ /usr/local/lib/

# Install oqto-browserd runtime dependencies and Playwright browser when assets are present
RUN if [ -f /usr/local/lib/oqto-browserd/package.json ]; then \
      cd /usr/local/lib/oqto-browserd && bun install --production; \
      mkdir -p /usr/local/share/playwright-browsers; \
      PLAYWRIGHT_BROWSERS_PATH=/usr/local/share/playwright-browsers \
        bun x playwright install --with-deps chromium; \
    fi


# --- Environment ---

ENV LANG=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8 \
    TERM=xterm-256color \
    SHELL=/bin/zsh \
    OQTO_DATA_DIR=/data \
    OQTO_PORT=8080 \
    EAVS_PORT=3033

# --- Create oqto user ---

# Remove default ubuntu user (UID/GID 1000) if present, then create oqto user
RUN userdel -r ubuntu 2>/dev/null || true \
    && groupadd --gid 1000 oqto 2>/dev/null || groupmod -n oqto "$(getent group 1000 | cut -d: -f1)" \
    && useradd --uid 1000 --gid 1000 -m -s /bin/zsh oqto \
    && echo "oqto ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers.d/oqto \
    && chmod 0440 /etc/sudoers.d/oqto

# --- Data directories ---

RUN mkdir -p \
    /data/oqto \
    /data/eavs \
    /data/users \
    /data/workspaces \
    /run/oqto \
    && chown -R oqto:oqto /data /run/oqto

# --- Caddy config (placeholder, overwritten by entrypoint) ---

RUN mkdir -p /etc/caddy
COPY deploy/docker/Caddyfile /etc/caddy/Caddyfile

# --- Entrypoint ---

COPY deploy/docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

EXPOSE 8080
VOLUME ["/data"]

ENTRYPOINT ["tini", "--"]
CMD ["/usr/local/bin/entrypoint.sh"]
