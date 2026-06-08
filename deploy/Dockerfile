# Stage 1: Build frontend
FROM node:20-alpine AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN corepack enable && pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm run build

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
COPY backend/ ./backend/
COPY --from=frontend-builder /app/frontend/dist/ ./frontend/dist/

WORKDIR /app/backend
RUN RUST_TARGET="$(cat /target.txt)" && \
    cargo build --release --target "$RUST_TARGET" && \
    cp "target/$RUST_TARGET/release/mailquill" /mailquill

# Stage 3: Runtime image
FROM alpine:3.20
RUN apk add --no-cache ca-certificates tzdata

RUN addgroup -S mailquill && \
    adduser -S -u 1000 -G mailquill mailquill

RUN mkdir /data && chown 1000:1000 /data

COPY --from=rust-builder /mailquill /usr/local/bin/mailquill

USER 1000
VOLUME ["/data"]
EXPOSE 8080
ENTRYPOINT ["mailquill", "serve"]
