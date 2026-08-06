# Development Setup

## Prerequisites

- Rust (stable, via rustup)
- Bun 1.2+
- Docker (for integration tests)

## Rust Musl Targets

For cross-compilation and reproducible musl builds, add the three targets:

```sh
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-unknown-linux-musl
rustup target add armv7-unknown-linux-musleabihf
```

On Debian/Ubuntu, also install the cross-linkers:

```sh
sudo apt-get install -y musl-tools gcc-aarch64-linux-gnu gcc-arm-linux-gnueabihf
```

## Build

```sh
make build          # frontend then backend
make build-frontend # frontend only
make build-backend  # backend only (requires frontend/dist/ to exist)
```

## Rust Workspace

The Rust workspace lives at the repository root. Run `cargo` commands from root.

```sh
cargo check         # fast type check
cargo test          # run all tests
cargo build --release --target x86_64-unknown-linux-musl
```

## Frontend

```sh
cd frontend
bun install
bun run dev         # dev server (Vite HMR)
bun run build       # production build into frontend/dist/
```

## Environment

Copy `deploy/.env.example` to `deploy/.env` and fill in the required values.

## Helm Install

```sh
# Install with default SQLite mode
helm install mailquill deploy/helm/mailquill \
  --namespace mailquill --create-namespace \
  --set ingress.enabled=true \
  --set ingress.hostname=mail.example.com \
  --set ingress.tls=true \
  --set image.tag=v0.1.0

# Required values (pass via --set or a secrets values file)
#   existingSecret=<name>   OR set in values: JWT_SECRET, VAPID_*, CREDENTIAL_ENCRYPTION_KEY

# Upgrade
helm upgrade mailquill deploy/helm/mailquill \
  --namespace mailquill \
  --set image.tag=v0.2.0

# Postgres mode
helm install mailquill deploy/helm/mailquill \
  --namespace mailquill --create-namespace \
  --set database.type=postgres \
  --set replicaCount=2
```
