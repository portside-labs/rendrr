# Multi-stage build: dependency caching via cargo-chef, then the binary, then
# a small Debian runtime with the libraries dxpdf links against at runtime.

FROM rust:1.98-bookworm AS chef
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# dxpdf links Skia (native C++) which needs clang + fontconfig/freetype headers.
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang \
    cmake \
    libfontconfig1-dev \
    libfreetype-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libfontconfig1 \
    libfreetype6 \
    fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home rendrr

COPY --from=builder /app/target/release/rendrr /usr/local/bin/rendrr

USER rendrr

ENV PORT=8080
EXPOSE 8080 8443

# /health needs no auth, so this works whether or not OAUTH_ISSUER is set.
# It only reports process liveness — it deliberately does not touch object
# storage, so a transient S3 outage won't get a healthy container restarted.
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -fsS "http://localhost:${PORT}/health" || exit 1

CMD ["rendrr"]
