# syntax=docker/dockerfile:1.7

ARG RUST_IMAGE=rust:1-trixie
ARG RUNTIME_IMAGE=debian:trixie-slim

FROM ${RUST_IMAGE} AS builder

WORKDIR /src

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      pkg-config \
      libssl-dev \
      libsqlite3-dev \
      libfuse3-dev \
      protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

COPY . .

RUN cargo build --release --locked \
      -p oqto \
      --bin oqto \
      --bin oqtoctl \
      --bin oqto-runner \
      --bin oqto-sandbox \
      --bin pi-bridge && \
    cargo build --release --locked \
      -p oqto-files \
      --bin oqto-files

FROM ${RUNTIME_IMAGE} AS runtime

ARG APP_USER=oqto

ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    RUST_LOG=info

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      ca-certificates \
      libssl3 \
      libsqlite3-0 \
      libfuse3-4 \
      tzdata \
    && rm -rf /var/lib/apt/lists/* && \
    useradd --system --create-home --shell /usr/sbin/nologin ${APP_USER}

COPY --from=builder /src/target/release/oqto /usr/local/bin/oqto
COPY --from=builder /src/target/release/oqtoctl /usr/local/bin/oqtoctl
COPY --from=builder /src/target/release/oqto-runner /usr/local/bin/oqto-runner
COPY --from=builder /src/target/release/oqto-sandbox /usr/local/bin/oqto-sandbox
COPY --from=builder /src/target/release/pi-bridge /usr/local/bin/pi-bridge
COPY --from=builder /src/target/release/oqto-files /usr/local/bin/oqto-files

USER ${APP_USER}
WORKDIR /home/${APP_USER}

ENTRYPOINT ["/usr/local/bin/oqto"]
CMD ["--help"]
