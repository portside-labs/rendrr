# Multi-stage build: dependency caching via cargo-chef, then the binary, then
# a small Debian runtime with the libraries dxpdf links against at runtime.

FROM rust:1.98-trixie AS chef
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# dxpdf links Skia (native C++). Beyond the clang/fontconfig/freetype headers,
# skia-bindings drives its own GN + ninja build, so both ninja and python3 have
# to be present or the build panics with "failed to run `ninja`". GitHub's
# hosted runners happen to ship ninja, so CI passes without it — only the
# container build fails, and only at release time.
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang \
    cmake \
    ninja-build \
    python3 \
    libfontconfig1-dev \
    libfreetype-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM debian:trixie-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    fontconfig \
    libfontconfig1 \
    libfreetype6 \
    fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /home/rendrr rendrr

COPY --from=builder /app/target/release/rendrr /usr/local/bin/rendrr

USER rendrr

# Skia asks fontconfig for fonts on every PDF render. Without a writable cache
# directory it logs "No writable cache directories" and rescans the font set
# each time the process starts, so give the service user a home and build the
# cache at image-build time instead of on the first request.
ENV XDG_CACHE_HOME=/home/rendrr/.cache
RUN fc-cache --force --system-only

ENV PORT=8080
EXPOSE 8080 8443

# /health needs no auth, so this works whether or not OAUTH_ISSUER is set.
# It only reports process liveness — it deliberately does not touch object
# storage, so a transient S3 outage won't get a healthy container restarted.
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -fsS "http://localhost:${PORT}/health" || exit 1

CMD ["rendrr"]
