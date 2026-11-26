# syntax=docker/dockerfile:1

# Stage 1: Build frontend
FROM oven/bun:1-alpine AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package.json frontend/bun.lock ./
RUN bun install --frozen-lockfile
COPY frontend/ ./
# --bun shims `node` to bun for the scripts invoked by the build pipeline.
RUN bun run --bun build

# Stage 2: Rust toolchain shared by the planner and the builder
FROM rust:1-alpine AS chef
RUN apk add --no-cache musl-dev sccache

ARG TARGETARCH
ARG TARGETVARIANT

# Select musl target based on platform
RUN case "${TARGETARCH}/${TARGETVARIANT:-}" in \
      "amd64/")   echo "x86_64-unknown-linux-musl"    > /target.txt ;; \
      "arm64/")   echo "aarch64-unknown-linux-musl"   > /target.txt ;; \
      "arm/v7")   echo "armv7-unknown-linux-musleabihf" > /target.txt ;; \
      *)          echo "x86_64-unknown-linux-musl"    > /target.txt ;; \
    esac

RUN rustup target add "$(cat /target.txt)"

# Runs a cargo command with sccache as the compiler cache. sccache stores each
# compiled crate under a hash of its inputs, so a crate is compiled once and
# then fetched, also after the image layers above it were rebuilt or evicted.
#
# Storage is the local directory /var/cache/sccache (a BuildKit cache mount)
# unless the build passes a secret `sccache_env`: a file of SCCACHE_* settings
# selecting one of sccache's remote backends (WebDAV, S3, Redis, …). A remote
# backend lives outside the builder, so nothing the builder evicts can empty
# it; deploy/docker-compose.sccache.yml sets one up locally. If the configured
# storage cannot be reached, the build continues without the cache.
COPY --chmod=755 <<'EOF' /usr/local/bin/with-sccache
#!/bin/sh
set -e
if [ -f /run/secrets/sccache_env ]; then
    set -a
    . /run/secrets/sccache_env
    set +a
fi
export SCCACHE_DIR=/var/cache/sccache
if sccache --start-server; then
    export RUSTC_WRAPPER=sccache
    "$@"
    sccache --show-stats
    # Stopping the server waits for pending cache writes.
    sccache --stop-server >/dev/null
else
    echo "with-sccache: cache storage unavailable, building without sccache" >&2
    "$@"
fi
EOF

# cargo-chef splits the build into "dependencies" and "workspace crates" so
# the dependencies land in an image layer of their own.
RUN --mount=type=secret,id=sccache_env \
    --mount=type=cache,id=sccache-${TARGETARCH}${TARGETVARIANT},target=/var/cache/sccache \
    with-sccache cargo install cargo-chef --locked --version 0.1.78

WORKDIR /app

# Stage 3: Reduce the workspace to a recipe that only changes when a manifest
# or the lockfile changes, not when source files do.
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY xtask/ ./xtask/
COPY backend/ ./backend/
RUN cargo chef prepare --recipe-path recipe.json

# Stage 4: Build Rust binary
FROM chef AS rust-builder

# Dependencies first, keyed on the recipe alone: a source change reuses this
# layer and recompiles only the workspace crates. The compiled dependencies
# are a plain layer rather than a cache-mounted target/ so that `cache-to`
# exports them (cache mounts never are). sccache backs the layer up: when it
# is gone, the dependencies are fetched from the compiler cache instead of
# being compiled again.
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=secret,id=sccache_env \
    --mount=type=cache,id=sccache-${TARGETARCH}${TARGETVARIANT},target=/var/cache/sccache \
    with-sccache cargo chef cook --release --locked --target "$(cat /target.txt)" -p api \
      --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY xtask/ ./xtask/
COPY backend/ ./backend/
COPY --from=frontend-builder /app/frontend/dist/ ./frontend/dist/

RUN --mount=type=secret,id=sccache_env \
    --mount=type=cache,id=sccache-${TARGETARCH}${TARGETVARIANT},target=/var/cache/sccache \
    RUST_TARGET="$(cat /target.txt)" && \
    with-sccache cargo build --release --locked --target "$RUST_TARGET" -p api && \
    cp "target/$RUST_TARGET/release/mailquill" /mailquill

# Stage 5: Runtime image
FROM alpine:3.24
RUN apk add --no-cache ca-certificates tzdata

RUN addgroup -S mailquill && \
    adduser -S -u 1000 -G mailquill mailquill

RUN mkdir /data && chown 1000:1000 /data

COPY --from=rust-builder /mailquill /usr/local/bin/mailquill

# Shipped (not compiled-in) phishing brand list. Operators can override it by
# editing /data/brands.json, which is seeded from this file on first start.
COPY --from=rust-builder /app/backend/phishing/brands.json /usr/local/share/mailquill/brands.json
ENV MAILQUILL_BRANDS_FILE=/usr/local/share/mailquill/brands.json

USER 1000
VOLUME ["/data"]
EXPOSE 8080
ENTRYPOINT ["mailquill"]
CMD ["serve"]
