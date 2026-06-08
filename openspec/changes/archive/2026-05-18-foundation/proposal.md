## Why

Before any user-facing feature can be built, Mailquill needs a compilable, deployable skeleton: a Rust workspace with correct crate boundaries, a React/Vite app wired to the build system, a database migration runner, a pluggable blob storage abstraction for message bodies and attachments, PII log redaction infrastructure, and a complete deployment pipeline. Every subsequent change depends on this foundation existing.

## What Changes

- Rust workspace with layered crate structure (`core/`, `api/`, `db/`, `imap-sync/`, `calendar-sync/`, `contact-sync/`, `smtp/`)
- React/Vite frontend scaffold wired to transport abstraction layer
- SQLite (per-user) + Postgres dual-mode migration runner
- `BlobStore` trait with local filesystem and S3-compatible backends, optional AES-256-GCM encryption wrapper
- `Pii<T>` newtype for configurable PII log redaction across all tracing call sites
- Single self-contained binary: `rust-embed` bundles frontend dist into the Axum binary
- Multi-arch container image (`linux/amd64`, `linux/arm64`, `linux/arm/v7`) via musl static linking
- Docker Compose (SQLite default + Postgres override), Helm chart, raw Kubernetes manifests
- GitHub Actions CI: lint, test, multi-arch buildx on tag push

## Capabilities

### New Capabilities

_(none — this change is pure infrastructure; no user-facing capabilities)_

### Modified Capabilities

_(none)_

## Impact

- New project: Rust workspace (`backend/`), React SPA (`frontend/`), `Makefile`, `.env.example`
- Rust dependencies: `axum`, `sqlx` (SQLite + Postgres), `tokio`, `serde`, `uuid`, `tracing`, `object_store` (local + aws features), `aes-gcm`, `zstd`, `sha2`, `rust-embed`, `tower`
- Frontend dependencies: React, shadcn/ui, Tailwind CSS, TanStack Query, React Router, Zustand, vite-plugin-pwa (stub manifest only in this change)
- Build: `Makefile` enforces `npm run build` → `cargo build --release` order so `frontend/dist/` exists for `rust-embed`
- Deployment: `deploy/Dockerfile` (3-stage musl), `deploy/docker-compose.yml`, `deploy/docker-compose.postgres.yml`, `deploy/helm/mailquill/`, `deploy/k8s/`
- CI: `.github/workflows/release.yml` — buildx + QEMU, publishes to `ghcr.io/mailquill/mailquill`
- No external runtime dependencies; all infra is self-contained
