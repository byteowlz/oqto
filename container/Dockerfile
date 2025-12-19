# Stage 1: Build fileserver and tmpltr
# Run from repository root: docker build -f container/Dockerfile .
FROM rust:latest AS rust-builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    git \
    && rm -rf /var/lib/apt/lists/*

# Build fileserver
COPY fileserver/Cargo.toml fileserver/Cargo.lock ./
COPY fileserver/src ./src
RUN cargo build --release

# Build tmpltr
RUN git clone --depth 1 https://github.com/byteowlz/tmpltr.git /build/tmpltr && \
    cd /build/tmpltr && \
    cargo build --release

# Stage 2: Main container
# Minimal Arch Linux container with opencode and development tools
# x86_64 only (Arch Linux does not support ARM64)
FROM archlinux:latest

# Build arguments
ARG USERNAME=dev
ARG USER_UID=1000
ARG USER_GID=1000
ARG OPENCODE_PORT=41820
ARG FILESERVER_PORT=41821
ARG TTYD_PORT=41822

# Environment variables
ENV LANG=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENV TERM=xterm-256color
ENV SHELL=/bin/zsh
ENV XDG_CONFIG_HOME=/home/${USERNAME}/.config
ENV XDG_DATA_HOME=/home/${USERNAME}/.local/share
ENV XDG_STATE_HOME=/home/${USERNAME}/.local/state
ENV XDG_CACHE_HOME=/home/${USERNAME}/.cache
ENV OPENCODE_PORT=${OPENCODE_PORT}
ENV FILESERVER_PORT=${FILESERVER_PORT}
ENV TTYD_PORT=${TTYD_PORT}
# EAVS_URL is set by orchestrator - points to host EAVS instance
ENV EAVS_URL=""

# Update system and install base packages
RUN pacman -Syu --noconfirm && \
  pacman -S --noconfirm \
  base-devel \
  git \
  curl \
  wget \
  unzip \
  openssh \
  sudo \
  ripgrep \
  fd \
  fzf \
  neovim \
  zoxide \
  zsh \
  starship \
  yazi \
  ffmpegthumbnailer \
  jq \
  poppler \
  imagemagick \
  python \
  python-pip \
  python-uv \
  nodejs \
  npm \
  go \
  rust \
  ttyd \
  && pacman -Scc --noconfirm

# Install bun
RUN curl -fsSL https://bun.sh/install | bash && \
  mv /root/.bun /usr/local/bun
ENV BUN_INSTALL=/usr/local/bun
ENV PATH="/usr/local/bun/bin:${PATH}"

# Copy fileserver binary from builder stage
# Note: EAVS runs on the host, not in containers - containers connect via EAVS_URL
COPY --from=rust-builder /build/target/release/fileserver /usr/local/bin/fileserver
RUN chmod +x /usr/local/bin/fileserver

# Copy tmpltr binary from builder stage
COPY --from=rust-builder /build/tmpltr/target/release/tmpltr /usr/local/bin/tmpltr
RUN chmod +x /usr/local/bin/tmpltr

# Install typst (required for tmpltr)
RUN pacman -S --noconfirm typst && pacman -Scc --noconfirm

# Install fonts for Typst PDF rendering
# Templates use Helvetica Neue (proprietary) - Liberation Sans is a metric-compatible replacement
# Inter is used by some templates and is available in Arch repos
RUN pacman -S --noconfirm \
    noto-fonts \
    noto-fonts-emoji \
    ttf-dejavu \
    ttf-liberation \
    ttf-roboto \
    gnu-free-fonts \
    inter-font \
    && pacman -Scc --noconfirm

# Create non-root user
RUN groupadd --gid ${USER_GID} ${USERNAME} && \
  useradd --uid ${USER_UID} --gid ${USER_GID} -m ${USERNAME} && \
  echo "${USERNAME} ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers.d/${USERNAME} && \
  chmod 0440 /etc/sudoers.d/${USERNAME}

# Create XDG directories
RUN mkdir -p /home/${USERNAME}/.config \
  /home/${USERNAME}/.local/share \
  /home/${USERNAME}/.local/state \
  /home/${USERNAME}/.cache \
  /home/${USERNAME}/workspace && \
  chown -R ${USERNAME}:${USERNAME} /home/${USERNAME}

# Switch to non-root user for opencode installation
USER ${USERNAME}
WORKDIR /home/${USERNAME}

# Create .zshrc before opencode installer (installer needs it to add PATH)
RUN touch /home/${USERNAME}/.zshrc

# Install opencode via the official installer
RUN curl -fsSL https://opencode.ai/install | bash

# Set zsh as default shell for user
RUN sudo usermod -s /bin/zsh ${USERNAME}

# Switch to root to stage opencode + defaults outside the home directory.
USER root

# Keep a stable opencode binary outside /home so it survives mounting /home/dev.
RUN mkdir -p /opt/opencode && \
  cp -a /home/${USERNAME}/.opencode/. /opt/opencode/ && \
  ln -sf /opt/opencode/bin/opencode /usr/local/bin/opencode && \
  chmod +x /usr/local/bin/opencode

# Ship default dotfiles inside the image so the entrypoint can populate an empty mounted home.
COPY container/skel/ /usr/local/share/skel/
RUN chmod -R a+rX /usr/local/share/skel

USER ${USERNAME}

# Add opencode to PATH (both the per-user install and the stable /usr/local/bin symlink)
ENV PATH="/usr/local/bin:/home/${USERNAME}/.opencode/bin:/home/${USERNAME}/.local/bin:${PATH}"

# Copy dotfiles from skel into the image for non-mounted dev use.
COPY --chown=${USERNAME}:${USERNAME} container/skel/.zshrc /home/${USERNAME}/.zshrc
COPY --chown=${USERNAME}:${USERNAME} container/skel/.config /home/${USERNAME}/.config
COPY --chown=${USERNAME}:${USERNAME} container/skel/.local /home/${USERNAME}/.local

# Copy entrypoint script to /usr/local/bin
USER root
COPY container/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh
USER ${USERNAME}

# Working directory
WORKDIR /home/${USERNAME}/workspace

# Expose ports for opencode serve, file server, and ttyd terminal
EXPOSE ${OPENCODE_PORT}
EXPOSE ${FILESERVER_PORT}
EXPOSE ${TTYD_PORT}

# Entrypoint (in /usr/local/bin so it's not overwritten by home mount)
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
