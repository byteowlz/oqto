# Stage 1: Build fileserver
# Run from repository root: docker build -f container/Dockerfile .
FROM rust:1.83-slim AS rust-builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Build fileserver
COPY fileserver/Cargo.toml fileserver/Cargo.lock ./
COPY fileserver/src ./src
RUN cargo build --release

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
ENV SHELL=/bin/bash
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
  neovim \
  zoxide \
  starship \
  yazi \
  ffmpegthumbnailer \
  jq \
  poppler \
  imagemagick \
  python \
  python-pip \
  nodejs \
  npm \
  go \
  ttyd \
  && pacman -Scc --noconfirm

# Copy fileserver binary from builder stage
# Note: EAVS runs on the host, not in containers - containers connect via EAVS_URL
COPY --from=rust-builder /build/target/release/fileserver /usr/local/bin/fileserver
RUN chmod +x /usr/local/bin/fileserver

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

# Install opencode via the official installer
RUN curl -fsSL https://opencode.ai/install | bash

# Add opencode to PATH
ENV PATH="/home/${USERNAME}/.opencode/bin:/home/${USERNAME}/.local/bin:${PATH}"

# Copy configuration files (from container directory in build context)
COPY --chown=${USERNAME}:${USERNAME} container/.config/ /home/${USERNAME}/.config/

# Configure shell with starship and zoxide
RUN echo 'eval "$(starship init bash)"' >> /home/${USERNAME}/.bashrc && \
  echo 'eval "$(zoxide init bash)"' >> /home/${USERNAME}/.bashrc && \
  echo 'alias vim="nvim"' >> /home/${USERNAME}/.bashrc && \
  echo 'alias vi="nvim"' >> /home/${USERNAME}/.bashrc

# fileserver binary is already installed from the builder stage above

# Copy entrypoint script (from container directory in build context)
COPY --chown=${USERNAME}:${USERNAME} container/entrypoint.sh /home/${USERNAME}/entrypoint.sh
RUN chmod +x /home/${USERNAME}/entrypoint.sh

# Working directory
WORKDIR /home/${USERNAME}/workspace

# Expose ports for opencode serve, file server, and ttyd terminal
EXPOSE ${OPENCODE_PORT}
EXPOSE ${FILESERVER_PORT}
EXPOSE ${TTYD_PORT}

# Entrypoint
ENTRYPOINT ["/home/dev/entrypoint.sh"]
