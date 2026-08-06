# syntax=docker/dockerfile:1

# Stage 1: Build frontend
FROM oven/bun:1-alpine AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package.json frontend/bun.lock ./
RUN bun install --frozen-lockfile
COPY frontend/ ./
# --bun shims `node` to bun for the scripts invoked by the build pipeline.
RUN bun run --bun build

# Stage 2: Build Rust binary
FROM rust:1-alpine AS rust-builder
RUN apk add --no-cache musl-dev

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

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY xtask/ ./xtask/
COPY backend/ ./backend/
COPY --from=frontend-builder /app/frontend/dist/ ./frontend/dist/

# Cache mounts keep the crate registry and compiled artefacts across builds,
# so a source change only recompiles the workspace crates, not every
# dependency. The binary is copied out because the mount vanishes after RUN.
# target/ is keyed per architecture; parallel multi-arch builds must not mix
# object files.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-target-${TARGETARCH}${TARGETVARIANT},target=/app/target \
    RUST_TARGET="$(cat /target.txt)" && \
    cargo build --release --locked --target "$RUST_TARGET" -p api && \
    cp "target/$RUST_TARGET/release/mailquill" /mailquill

# Stage 3: Runtime image
FROM alpine:3.20
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
