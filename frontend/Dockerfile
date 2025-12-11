# syntax=docker/dockerfile:1.6
FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      bash \
      build-essential \
      ca-certificates \
      cargo \
      curl \
      fd-find \
      git \
      iputils-ping \
      libssl-dev \
      neovim \
      nodejs \
      npm \
      pandoc \
      poppler-utils \
      procps \
      python3 \
      python3-pip \
      python3-setuptools \
      python3-venv \
      ripgrep \
      tmux \
      ttyd \
      unzip \
      wget \
      zsh \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @opencode-ai/cli

RUN cargo install --locked yazi-cli && \
    mv /root/.cargo/bin/yazi /usr/local/bin/yazi && \
    rm -rf /root/.cargo

RUN ln -s /usr/bin/fdfind /usr/local/bin/fd

RUN useradd -ms /bin/bash agent && \
    mkdir -p /workspace && \
    chown -R agent:agent /workspace

COPY scripts/entrypoint.sh /usr/local/bin/start-services.sh
RUN chmod +x /usr/local/bin/start-services.sh

WORKDIR /workspace

EXPOSE 4096 9000 9090

VOLUME ["/workspace"]

USER agent

ENTRYPOINT ["/usr/local/bin/start-services.sh"]
